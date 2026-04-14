//! Windows process spawning via `CreateProcessW` with `lpReserved2` for FD 3+.
//!
//! The `lpReserved2` field in `STARTUPINFOW` encodes the MSVC CRT file
//! descriptor table. This allows child processes (that use the MSVC CRT) to
//! inherit arbitrary file descriptors, not just stdin/stdout/stderr.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle, RawHandle};

use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
};

use super::{ChildEx, ChildInner, CommandEx, Fd, SendHandle, SendHpcon};

/// CRT fd flags used in the lpReserved2 buffer.
const FOPEN: u8 = 0x01;
const FPIPE: u8 = 0x08;

/// Resolve `cmd.path` via PATH + PATHEXT if it's a bare command name.
///
/// Equivalent of Unix `posix_spawnp`'s PATH search. `CreateProcessW` does not
/// search PATH when `lpApplicationName` is non-NULL, so we must do it ourselves.
fn resolve_path_if_needed(cmd: &mut CommandEx) {
    let name = cmd.path.to_string_lossy();
    if name.contains('/') || name.contains('\\') {
        return;
    }

    // Use PATH from cmd.env (child's environment) first, falling back to the
    // process environment. This matches posix_spawnp on Unix, which searches
    // the calling process's PATH rather than the child's envp.
    let env_path = cmd
        .env
        .get(OsStr::new("PATH"))
        .and_then(|v| v.to_str())
        .map(String::from);
    let proc_path;
    let path_var = match &env_path {
        Some(p) => p.as_str(),
        None => {
            proc_path = std::env::var("PATH").unwrap_or_default();
            proc_path.as_str()
        }
    };

    let pathext = cmd.env.get(OsStr::new("PATHEXT")).and_then(|v| v.to_str());
    if let Some(resolved) = super::resolve_windows::resolve_command(cmd.path.as_ref(), path_var, pathext) {
        cmd.path = resolved.into_os_string();
    }
}

/// Spawn a child process using `CreateProcessW` with full fd table support.
///
/// If any fd is `Fd::Pty`, delegates to `spawn_with_conpty` for terminal
/// emulation. Mixed cases (only stdout or only stderr needing a PTY) are
/// handled by ConPTY with explicit `hStd*` overrides for the non-PTY fd.
pub(super) fn spawn_impl(mut cmd: CommandEx) -> io::Result<ChildEx> {
    resolve_path_if_needed(&mut cmd);

    let any_pty = cmd.fds.values().any(|fd| matches!(fd, Fd::Pty));
    if any_pty {
        return spawn_with_conpty(cmd);
    }

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
            Fd::Pty => unreachable!("Pty fds are handled by spawn_with_conpty"),
        }
    }

    // Build STARTUPINFOW.
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;

    // Set standard handles if specified in the fd table. For handles not
    // in the table, fall back to the parent's current std handles via
    // GetStdHandle (not INVALID_HANDLE_VALUE, which would leave the child
    // with a broken handle).
    if handle_table.contains_key(&0) || handle_table.contains_key(&1) || handle_table.contains_key(&2) {
        use windows::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};

        si.dwFlags |= STARTF_USESTDHANDLES;
        si.hStdInput = handle_table
            .get(&0)
            .map(|h| h.0)
            .unwrap_or_else(|| unsafe { GetStdHandle(STD_INPUT_HANDLE) }.unwrap_or(INVALID_HANDLE_VALUE));
        si.hStdOutput = handle_table
            .get(&1)
            .map(|h| h.0)
            .unwrap_or_else(|| unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.unwrap_or(INVALID_HANDLE_VALUE));
        si.hStdError = handle_table
            .get(&2)
            .map(|h| h.0)
            .unwrap_or_else(|| unsafe { GetStdHandle(STD_ERROR_HANDLE) }.unwrap_or(INVALID_HANDLE_VALUE));
    }

    // Build lpReserved2 for FDs 3+ (CRT fd table).
    let reserved2 = build_lpreserved2(&handle_table);
    if !reserved2.is_empty() {
        si.cbReserved2 =
            u16::try_from(reserved2.len()).expect("lpReserved2 buffer exceeds u16::MAX — fd number too large");
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

    // Close all parent-side handles that were given to the child (write-ends of
    // Fd::Pipe/InputPipe and cloned Fd::File handles). The parent retains the
    // opposite ends via the `pipes` map.
    for &(handle, _) in handle_table.values() {
        let _ = unsafe { CloseHandle(handle) };
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
        inner: ChildInner::Handle(SendHandle(pi.hProcess)),
        pipes,
        _conpty_input: None,
        conpty_output_fd: None,
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
    // Environment block must be sorted case-insensitively per Windows convention.
    entries.sort_by(|a, b| {
        a.iter()
            .map(|&c| {
                if (b'a' as u16..=b'z' as u16).contains(&c) {
                    c - 32
                } else {
                    c
                }
            })
            .cmp(b.iter().map(|&c| {
                if (b'a' as u16..=b'z' as u16).contains(&c) {
                    c - 32
                } else {
                    c
                }
            }))
    });

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

// ConPTY spawn ========================================================================================================

/// Spawn a child process using ConPTY (Windows Pseudo Console) for terminal
/// emulation. The child sees a real console, so `isatty()` returns true for
/// fds handled by the ConPTY.
///
/// Supports three modes:
/// - **Both stdout and stderr are Pty**: ConPTY handles both. Output is merged
///   into a single stream (stored as fd 1). This is invisible to the user since
///   both streams go to the same terminal.
/// - **Only stdout is Pty**: ConPTY output is stored as fd 1. stderr gets an
///   explicit pipe via `hStdError`, keeping streams separate.
/// - **Only stderr is Pty**: ConPTY output is stored as fd 2. stdout gets an
///   explicit pipe via `hStdOutput`, keeping streams separate.
///
/// Non-Pty fds (File, Pipe, InputPipe) are passed via lpReserved2 as usual.
fn spawn_with_conpty(cmd: CommandEx) -> io::Result<ChildEx> {
    use windows::Win32::System::Console::{ClosePseudoConsole, CreatePseudoConsole, HPCON};
    use windows::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
        CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
        STARTUPINFOEXW,
    };

    // Get terminal size from the parent console (default 80x24 if unavailable).
    let size = get_console_size();

    // Create input/output pipes for the ConPTY.
    let (pty_input_read, pty_input_write) = create_pipe()?;
    let (pty_output_read, pty_output_write) = create_pipe()?;

    // Create the pseudo console.
    let hpc = unsafe { CreatePseudoConsole(size, pty_input_read, pty_output_write, 0) }
        .map_err(|e| io::Error::other(format!("CreatePseudoConsole failed: {e}")))?;

    // Close pipe ends that ConPTY now owns copies of.
    let _ = unsafe { CloseHandle(pty_input_read) };
    let _ = unsafe { CloseHandle(pty_output_write) };
    // Keep pty_input_write alive — closing it while the child runs makes ConPTY
    // generate a close event (STATUS_CONTROL_C_EXIT). Convert to a File so it
    // drops with the ChildEx pipes map. Use fd -1 as a sentinel (not a real fd).
    let pty_input_file = unsafe { File::from_raw_handle(pty_input_write.0 as _) };

    /// RAII guard that calls `DeleteProcThreadAttributeList` on drop.
    struct AttrListGuard(LPPROC_THREAD_ATTRIBUTE_LIST);
    impl Drop for AttrListGuard {
        fn drop(&mut self) {
            unsafe { DeleteProcThreadAttributeList(self.0) };
        }
    }

    // Build the attribute list. Allocate for 2 attributes: PSEUDOCONSOLE is
    // always present; HANDLE_LIST is added later if non-Pty fds need inheritance.
    let mut attr_size: usize = 0;
    let _ = unsafe { InitializeProcThreadAttributeList(None, 2, Some(0), &mut attr_size) };
    let mut attr_buf = vec![0u8; attr_size];
    let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as _);
    unsafe { InitializeProcThreadAttributeList(Some(attr_list), 2, Some(0), &mut attr_size) }
        .map_err(|e| io::Error::other(format!("InitializeProcThreadAttributeList: {e}")))?;
    let _attr_guard = AttrListGuard(attr_list);

    // PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE = 0x00020016
    const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;
    unsafe {
        UpdateProcThreadAttribute(
            attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            Some(hpc.0 as *const std::ffi::c_void),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        )
    }
    .map_err(|e| io::Error::other(format!("UpdateProcThreadAttribute: {e}")))?;

    // Determine which of stdout/stderr are handled by ConPTY vs pipes.
    let stdout_pty = matches!(cmd.fds.get(&1), Some(Fd::Pty));
    debug_assert!(
        stdout_pty || matches!(cmd.fds.get(&2), Some(Fd::Pty)),
        "spawn_with_conpty called but neither stdout nor stderr is Pty"
    );

    // For non-Pty fds that were requested as Pipe or other types, create
    // regular pipes. Also, for stdout/stderr that are NOT Pty, we need an
    // explicit pipe so we can set hStdOutput/hStdError to override ConPTY.
    let mut pipes: HashMap<i32, File> = HashMap::new();
    let mut handle_table: HashMap<i32, (HANDLE, u8)> = HashMap::new();

    for (&fd_num, fd_spec) in &cmd.fds {
        match fd_spec {
            Fd::Pty => {} // Handled by ConPTY
            Fd::Pipe => {
                let (read_handle, write_handle) = create_pipe()?;
                let read_file = unsafe { File::from_raw_handle(read_handle.0 as _) };
                make_inheritable(write_handle)?;
                handle_table.insert(fd_num, (write_handle, FOPEN | FPIPE));
                pipes.insert(fd_num, read_file);
            }
            Fd::InputPipe => {
                let (read_handle, write_handle) = create_pipe()?;
                let write_file = unsafe { File::from_raw_handle(write_handle.0 as _) };
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

    // Collect non-Pty handles that need to be inherited by the child.
    // With ConPTY, bInheritHandles is normally false (ConPTY provides console
    // handles directly). But when non-Pty fds exist (e.g. piped stdin from a
    // pipeline, or fds 3+ from redirections), we need bInheritHandles=true with
    // PROC_THREAD_ATTRIBUTE_HANDLE_LIST to whitelist exactly those handles.
    let inherit_handles: Vec<HANDLE> = handle_table.values().map(|(h, _)| *h).collect();
    let has_inheritable = !inherit_handles.is_empty();

    if has_inheritable {
        const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x00020002;
        unsafe {
            UpdateProcThreadAttribute(
                attr_list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                Some(inherit_handles.as_ptr() as *const std::ffi::c_void),
                inherit_handles.len() * std::mem::size_of::<HANDLE>(),
                None,
                None,
            )
        }
        .map_err(|e| io::Error::other(format!("UpdateProcThreadAttribute(HANDLE_LIST): {e}")))?;
    }

    // Build STARTUPINFOEXW.
    let mut si_ex: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si_ex.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si_ex.lpAttributeList = attr_list;

    // STARTF_USESTDHANDLES lets us selectively override individual std handles.
    // Handles left as null default to the ConPTY console. Handles set to an
    // explicit pipe bypass ConPTY for that fd, keeping streams separate.
    // See: https://github.com/microsoft/terminal/issues/4380#issuecomment-580865346
    si_ex.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;
    if let Some(&(h, _)) = handle_table.get(&0) {
        si_ex.StartupInfo.hStdInput = h;
    }
    if let Some(&(h, _)) = handle_table.get(&1) {
        si_ex.StartupInfo.hStdOutput = h;
    }
    if let Some(&(h, _)) = handle_table.get(&2) {
        si_ex.StartupInfo.hStdError = h;
    }

    // lpReserved2 for FDs 3+.
    let reserved2 = build_lpreserved2(&handle_table);
    if !reserved2.is_empty() {
        si_ex.StartupInfo.cbReserved2 =
            u16::try_from(reserved2.len()).expect("lpReserved2 buffer exceeds u16::MAX — fd number too large");
        si_ex.StartupInfo.lpReserved2 = reserved2.as_ptr() as *mut u8;
    }

    // Build command line, environment, path, cwd.
    let cmdline = cmd.commandline();
    let mut cmdline_wide: Vec<u16> = cmdline.encode_wide().chain(std::iter::once(0)).collect();
    let env_block = build_env_block(&cmd.env);
    let path_wide: Vec<u16> = cmd.path.encode_wide().chain(std::iter::once(0)).collect();
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
            None,
            None,
            has_inheritable, // Only inherit when HANDLE_LIST whitelists specific handles.
            CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            Some(env_block.as_ptr() as _),
            cwd_pcwstr,
            &si_ex.StartupInfo,
            &mut pi,
        )
    };

    // Cleanup: close child-side handles for non-Pty fds.
    for &(handle, _) in handle_table.values() {
        let _ = unsafe { CloseHandle(handle) };
    }

    result.map_err(|e| {
        // ConPTY must be closed on error too.
        unsafe { ClosePseudoConsole(hpc) };
        let win32_code = (e.code().0 as u32) & 0xFFFF;
        let kind = match win32_code {
            2 => io::ErrorKind::NotFound,
            3 => io::ErrorKind::NotFound,
            5 => io::ErrorKind::PermissionDenied,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, e)
    })?;

    let _ = unsafe { CloseHandle(pi.hThread) };

    // ConPTY output pipe carries console output. Store it under the fd that
    // is handled by ConPTY. When both are Pty, fd 1 gets the merged stream.
    // When only one is Pty, it gets the ConPTY output; the other has its own pipe.
    let pty_output_file = unsafe { File::from_raw_handle(pty_output_read.0 as _) };
    if stdout_pty {
        pipes.insert(1, pty_output_file);
    } else {
        pipes.insert(2, pty_output_file);
    }

    // Record which fd carries ConPTY output for clean_conpty_output routing.
    let conpty_fd = if stdout_pty { 1 } else { 2 };

    // Keep ConPTY alive until the child exits — closing it earlier tears down
    // the console session and can crash the child during initialization.
    Ok(ChildEx {
        inner: ChildInner::HandleWithPty(SendHandle(pi.hProcess), SendHpcon(hpc)),
        pipes,
        _conpty_input: Some(pty_input_file),
        conpty_output_fd: Some(conpty_fd),
    })
}

/// Get the current console window size, defaulting to 80x24.
fn get_console_size() -> windows::Win32::System::Console::COORD {
    use windows::Win32::System::Console::{
        GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO, COORD, STD_OUTPUT_HANDLE,
    };
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.unwrap_or(INVALID_HANDLE_VALUE);
    if unsafe { GetConsoleScreenBufferInfo(handle, &mut info) }.is_ok() {
        COORD {
            X: info.srWindow.Right - info.srWindow.Left + 1,
            Y: info.srWindow.Bottom - info.srWindow.Top + 1,
        }
    } else {
        COORD { X: 80, Y: 24 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_labels::EXEC;

    skuld::default_labels!(EXEC);

    /// Environment block sorting must be case-insensitive per Windows convention.
    #[skuld::test]
    fn env_block_sort_is_case_insensitive() {
        let mut env = HashMap::new();
        // "path" sorts after "PATH" in byte order but should be adjacent case-insensitively.
        env.insert(OsString::from("Zebra"), OsString::from("z"));
        env.insert(OsString::from("alpha"), OsString::from("a"));
        env.insert(OsString::from("PATH"), OsString::from("p1"));
        env.insert(OsString::from("path"), OsString::from("p2"));
        env.insert(OsString::from("Beta"), OsString::from("b"));

        let block = build_env_block(&env);
        // Decode the block back into entries.
        let entries: Vec<String> = block
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect();

        // Extract just the var names for order checking.
        let names: Vec<&str> = entries.iter().map(|e| e.split('=').next().unwrap()).collect();
        // Case-insensitive sort: alpha, Beta, PATH, path, Zebra (or path before PATH — both valid).
        for i in 1..names.len() {
            assert!(
                names[i - 1].to_ascii_uppercase() <= names[i].to_ascii_uppercase(),
                "env block not case-insensitively sorted: {:?} should come before {:?} (full order: {:?})",
                names[i - 1],
                names[i],
                names
            );
        }
    }
}
