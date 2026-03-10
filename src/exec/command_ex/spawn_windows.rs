//! Windows process spawning via `CreateProcessW` with `lpReserved2` for FD 3+.
//!
//! The `lpReserved2` field in `STARTUPINFOW` encodes the MSVC CRT file
//! descriptor table. This allows child processes (that use the MSVC CRT) to
//! inherit arbitrary file descriptors, not just stdin/stdout/stderr.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle, RawHandle};

use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
};

use super::{ChildEx, ChildInner, CommandEx, Fd};

/// CRT fd flags used in the lpReserved2 buffer.
const FOPEN: u8 = 0x01;
const FPIPE: u8 = 0x08;

/// Spawn a child process using `CreateProcessW` with full fd table support.
pub(super) fn spawn_impl(cmd: CommandEx) -> io::Result<ChildEx> {
    let mut pipes: HashMap<i32, File> = HashMap::new();
    let mut handle_table: HashMap<i32, (HANDLE, u8)> = HashMap::new();

    // Process the fd table: create pipes and collect handles.
    for (&fd_num, fd_spec) in &cmd.fds {
        match fd_spec {
            Fd::Pipe => {
                let (read_handle, write_handle) = create_pipe()?;
                let read_file = unsafe { File::from_raw_handle(read_handle.0 as _) };
                // Child gets write end; parent gets read end.
                make_inheritable(write_handle)?;
                handle_table.insert(fd_num, (write_handle, FOPEN | FPIPE));
                pipes.insert(fd_num, read_file);
            }
            Fd::InputPipe => {
                let (read_handle, write_handle) = create_pipe()?;
                let write_file = unsafe { File::from_raw_handle(write_handle.0 as _) };
                // Child gets read end; parent gets write end.
                make_inheritable(read_handle)?;
                handle_table.insert(fd_num, (read_handle, FOPEN | FPIPE));
                pipes.insert(fd_num, write_file);
            }
            Fd::File(file) => {
                let raw = file.try_clone()?.into_raw_handle();
                let handle = HANDLE(raw as _);
                make_inheritable(handle)?;
                handle_table.insert(fd_num, (handle, FOPEN));
            }
        }
    }

    // Build STARTUPINFOW.
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;

    // Set standard handles if specified in the fd table.
    if handle_table.contains_key(&0) || handle_table.contains_key(&1) || handle_table.contains_key(&2) {
        si.dwFlags |= STARTF_USESTDHANDLES;
        si.hStdInput = handle_table.get(&0).map(|h| h.0).unwrap_or(INVALID_HANDLE_VALUE);
        si.hStdOutput = handle_table.get(&1).map(|h| h.0).unwrap_or(INVALID_HANDLE_VALUE);
        si.hStdError = handle_table.get(&2).map(|h| h.0).unwrap_or(INVALID_HANDLE_VALUE);
    }

    // Build lpReserved2 for FDs 3+ (CRT fd table).
    let reserved2 = build_lpreserved2(&handle_table);
    if !reserved2.is_empty() {
        si.cbReserved2 = reserved2.len() as u16;
        // SAFETY: reserved2 lives until CreateProcessW returns.
        si.lpReserved2 = reserved2.as_ptr() as *mut u8;
    }

    // Build command line string.
    let cmdline = cmd.commandline();
    let mut cmdline_wide: Vec<u16> = cmdline.encode_wide().chain(std::iter::once(0)).collect();

    // Build environment block.
    let env_block = build_env_block(&cmd.env);

    // Build path (null-terminated wide string).
    let path_wide: Vec<u16> = cmd.path.encode_wide().chain(std::iter::once(0)).collect();

    // Build cwd (null-terminated wide string, or null).
    let cwd_wide: Option<Vec<u16>> = cmd
        .cwd
        .as_ref()
        .map(|p| p.as_os_str().encode_wide().chain(std::iter::once(0)).collect());

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    let cwd_pcwstr = match &cwd_wide {
        Some(w) => windows::core::PCWSTR(w.as_ptr()),
        None => windows::core::PCWSTR::null(),
    };

    let result = unsafe {
        CreateProcessW(
            windows::core::PCWSTR(path_wide.as_ptr()),
            Some(windows::core::PWSTR(cmdline_wide.as_mut_ptr())),
            None, // process security attributes
            None, // thread security attributes
            true, // inherit handles
            CREATE_UNICODE_ENVIRONMENT,
            Some(env_block.as_ptr() as _),
            cwd_pcwstr,
            &si,
            &mut pi,
        )
    };

    // Close write-ends of pipes in the parent.
    for (&fd_num, &(handle, _)) in &handle_table {
        if pipes.contains_key(&fd_num) {
            // This was a Pipe fd — close the write end we gave to the child.
            let _ = unsafe { CloseHandle(handle) };
        }
    }

    result.map_err(|e| {
        // Extract the Win32 error code from the HRESULT (low 16 bits).
        let win32_code = (e.code().0 as u32) & 0xFFFF;
        let kind = match win32_code {
            2 => io::ErrorKind::NotFound,         // ERROR_FILE_NOT_FOUND
            3 => io::ErrorKind::NotFound,         // ERROR_PATH_NOT_FOUND
            5 => io::ErrorKind::PermissionDenied, // ERROR_ACCESS_DENIED
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, e)
    })?;

    // Close the thread handle (we don't need it).
    let _ = unsafe { CloseHandle(pi.hThread) };

    Ok(ChildEx {
        inner: ChildInner::Handle(pi.hProcess),
        pipes,
    })
}

/// Build the `lpReserved2` buffer encoding the CRT fd table.
///
/// Format:
/// ```text
/// [u32: fd_count]
/// [u8 * fd_count: flags for each fd]
/// [HANDLE * fd_count: OS handle for each fd]
/// ```
fn build_lpreserved2(handles: &HashMap<i32, (HANDLE, u8)>) -> Vec<u8> {
    if handles.is_empty() {
        return Vec::new();
    }

    let max_fd = handles.keys().copied().max().unwrap_or(0);
    let fd_count = (max_fd + 1) as usize;

    // Only build the buffer if there are FDs above 2 (stdio is handled via STARTUPINFO).
    if max_fd < 3 {
        return Vec::new();
    }

    let handle_size = std::mem::size_of::<RawHandle>();
    let buf_size = 4 + fd_count + fd_count * handle_size;
    let mut buf = vec![0u8; buf_size];

    // Write fd count.
    buf[0..4].copy_from_slice(&(fd_count as u32).to_le_bytes());

    // Write flags.
    for (&fd, &(_, flags)) in handles {
        if (fd as usize) < fd_count {
            buf[4 + fd as usize] = flags;
        }
    }

    // Write handles.
    let handles_offset = 4 + fd_count;
    for (&fd, &(handle, _)) in handles {
        if (fd as usize) < fd_count {
            let offset = handles_offset + (fd as usize) * handle_size;
            let handle_bytes = (handle.0 as usize).to_le_bytes();
            buf[offset..offset + handle_size].copy_from_slice(&handle_bytes[..handle_size]);
        }
    }

    buf
}

/// Build a Windows environment block: sorted `KEY=VALUE\0` pairs, double-null terminated.
fn build_env_block(env: &HashMap<OsString, OsString>) -> Vec<u16> {
    let mut entries: Vec<Vec<u16>> = env
        .iter()
        .map(|(k, v)| {
            let mut entry: Vec<u16> = k.encode_wide().collect();
            entry.push(b'=' as u16);
            entry.extend(v.encode_wide());
            entry.push(0);
            entry
        })
        .collect();
    // Environment block must be sorted (case-insensitive) per Windows convention.
    entries.sort();

    let mut block: Vec<u16> = Vec::new();
    for entry in entries {
        block.extend(entry);
    }
    block.push(0); // double-null terminator
    block
}

/// Create a pipe, returning (read_handle, write_handle).
fn create_pipe() -> io::Result<(HANDLE, HANDLE)> {
    let mut read_handle = HANDLE::default();
    let mut write_handle = HANDLE::default();
    unsafe { CreatePipe(&mut read_handle, &mut write_handle, None, 0) }.map_err(io::Error::other)?;
    Ok((read_handle, write_handle))
}

/// Mark a handle as inheritable by child processes.
fn make_inheritable(handle: HANDLE) -> io::Result<()> {
    use windows::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
    unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }.map_err(io::Error::other)
}
