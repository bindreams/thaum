//! Redirect resolution: opens files, sets up FD overrides, and applies them
//! to an `IoContext` for the duration of a single command.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};

use crate::ast::{Redirect, RedirectKind};
use crate::exec::error::ExecError;
use crate::exec::expand;
use crate::exec::io_context::IoContext;
use crate::exec::Executor;

/// Temporary redirect state for a single command.
///
/// Holds file handles opened by the command's redirect list. FDs 0-2 override
/// the IoContext; FDs 3+ are stored in `extra_fds` for dup resolution and
/// child process inheritance.
pub(super) struct ActiveRedirects {
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
    pub extra_fds: HashMap<i32, File>,
    /// FDs explicitly closed via `N>&-` / `N<&-`. Used by `exec` redirect-only
    /// mode to remove persistent FDs from the fd_table.
    pub closed_fds: HashSet<i32>,
}

impl ActiveRedirects {
    pub fn new() -> Self {
        ActiveRedirects {
            stdin: None,
            stdout: None,
            stderr: None,
            extra_fds: HashMap::new(),
            closed_fds: HashSet::new(),
        }
    }

    /// Returns true if any redirections are active.
    #[allow(dead_code)] // Kept for diagnostics and future use.
    pub fn is_active(&self) -> bool {
        self.stdin.is_some() || self.stdout.is_some() || self.stderr.is_some() || !self.extra_fds.is_empty()
    }

    /// Build an IoContext that uses redirect file handles for FDs 0-2 where
    /// present, falling back to the original `io` streams.
    pub fn apply_to_io<'a>(&'a mut self, io: &'a mut IoContext<'_>) -> IoContext<'a> {
        let IoContext { stdin, stdout, stderr } = io;
        IoContext::new(
            match self.stdin.as_mut() {
                Some(f) => f as &mut dyn Read,
                None => *stdin,
            },
            match self.stdout.as_mut() {
                Some(f) => f as &mut dyn Write,
                None => *stdout,
            },
            match self.stderr.as_mut() {
                Some(f) => f as &mut dyn Write,
                None => *stderr,
            },
        )
    }
}

impl Executor {
    /// Process a command's redirect list into an `ActiveRedirects`.
    ///
    /// Redirects are processed left-to-right. `>&N` resolves against FDs
    /// already opened in this redirect list, then against the persistent
    /// fd_table.
    pub(super) fn resolve_redirects(&mut self, redirects: &[Redirect]) -> Result<ActiveRedirects, ExecError> {
        let mut active = ActiveRedirects::new();

        for redirect in redirects {
            let fd = redirect.fd;
            match &redirect.kind {
                RedirectKind::Input(word) => {
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = File::open(&resolved).map_err(|e| ExecError::BadRedirect(format!("{}: {}", path, e)))?;
                    assign_read_fd(&mut active, fd.unwrap_or(0), file)?;
                }
                RedirectKind::Output(word) | RedirectKind::Clobber(word) => {
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file =
                        File::create(&resolved).map_err(|e| ExecError::BadRedirect(format!("{}: {}", path, e)))?;
                    assign_write_fd(&mut active, fd.unwrap_or(1), file)?;
                }
                RedirectKind::Append(word) => {
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&resolved)
                        .map_err(|e| ExecError::BadRedirect(format!("{}: {}", path, e)))?;
                    assign_write_fd(&mut active, fd.unwrap_or(1), file)?;
                }
                RedirectKind::ReadWrite(word) => {
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(&resolved)
                        .map_err(|e| ExecError::BadRedirect(format!("{}: {}", path, e)))?;
                    assign_read_fd(&mut active, fd.unwrap_or(0), file)?;
                }
                RedirectKind::DupOutput(word) => {
                    let target = expand::expand_word(word, &mut self.env)?;
                    let dest_fd = fd.unwrap_or(1);
                    if target == "-" {
                        // Close the FD: use sink for 0-2, remove for 3+.
                        close_write_fd(&mut active, dest_fd);
                    } else if let Ok(src_fd) = target.parse::<i32>() {
                        let cloned = clone_fd_for_write(&active, &self.fd_table, src_fd)?;
                        assign_write_fd(&mut active, dest_fd, cloned)?;
                    } else {
                        return Err(ExecError::BadRedirect(format!("{}: ambiguous redirect", target)));
                    }
                }
                RedirectKind::DupInput(word) => {
                    let target = expand::expand_word(word, &mut self.env)?;
                    let dest_fd = fd.unwrap_or(0);
                    if target == "-" {
                        close_read_fd(&mut active, dest_fd);
                    } else if let Ok(src_fd) = target.parse::<i32>() {
                        let cloned = clone_fd_for_read(&active, &self.fd_table, src_fd)?;
                        assign_read_fd(&mut active, dest_fd, cloned)?;
                    } else {
                        return Err(ExecError::BadRedirect(format!("{}: ambiguous redirect", target)));
                    }
                }
                RedirectKind::HereDoc { body, .. } => {
                    // Create a temporary file with the heredoc body, use as stdin.
                    let mut tmpfile = tempfile()?;
                    tmpfile.write_all(body.as_bytes()).map_err(ExecError::Io)?;
                    tmpfile.seek_to_start()?;
                    assign_read_fd(&mut active, fd.unwrap_or(0), tmpfile)?;
                }
                RedirectKind::BashHereString(word) => {
                    let expanded = expand::expand_word(word, &mut self.env)?;
                    let mut tmpfile = tempfile()?;
                    tmpfile.write_all(expanded.as_bytes()).map_err(ExecError::Io)?;
                    tmpfile.write_all(b"\n").map_err(ExecError::Io)?;
                    tmpfile.seek_to_start()?;
                    assign_read_fd(&mut active, fd.unwrap_or(0), tmpfile)?;
                }
                RedirectKind::BashOutputAll(word) => {
                    // &> file — redirect both stdout and stderr to file
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file =
                        File::create(&resolved).map_err(|e| ExecError::BadRedirect(format!("{}: {}", path, e)))?;
                    let clone = file.try_clone().map_err(ExecError::Io)?;
                    active.stdout = Some(file);
                    active.stderr = Some(clone);
                }
                RedirectKind::BashAppendAll(word) => {
                    // &>> file — append both stdout and stderr to file
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&resolved)
                        .map_err(|e| ExecError::BadRedirect(format!("{}: {}", path, e)))?;
                    let clone = file.try_clone().map_err(ExecError::Io)?;
                    active.stdout = Some(file);
                    active.stderr = Some(clone);
                }
            }
        }

        Ok(active)
    }
}

/// Assign a file to the appropriate read FD slot.
fn assign_read_fd(active: &mut ActiveRedirects, fd: i32, file: File) -> Result<(), ExecError> {
    match fd {
        0 => active.stdin = Some(file),
        n => {
            active.extra_fds.insert(n, file);
        }
    }
    Ok(())
}

/// Assign a file to the appropriate write FD slot.
fn assign_write_fd(active: &mut ActiveRedirects, fd: i32, file: File) -> Result<(), ExecError> {
    match fd {
        1 => active.stdout = Some(file),
        2 => active.stderr = Some(file),
        n => {
            active.extra_fds.insert(n, file);
        }
    }
    Ok(())
}

/// Close a write FD by assigning a sink.
fn close_write_fd(active: &mut ActiveRedirects, fd: i32) {
    active.closed_fds.insert(fd);
    match fd {
        // For FDs 0-2, we can't truly close them — use /dev/null equivalent.
        // The null device file is opened lazily when needed via apply_to_io.
        // For now, just mark them as "will be null" by leaving as None.
        // The caller handles the default fallback.
        1 => active.stdout = None,
        2 => active.stderr = None,
        n => {
            active.extra_fds.remove(&n);
        }
    }
}

/// Close a read FD.
fn close_read_fd(active: &mut ActiveRedirects, fd: i32) {
    active.closed_fds.insert(fd);
    match fd {
        0 => active.stdin = None,
        n => {
            active.extra_fds.remove(&n);
        }
    }
}

/// Clone a file descriptor from the active redirects or persistent fd_table
/// for use as a write target.
fn clone_fd_for_write(active: &ActiveRedirects, fd_table: &HashMap<i32, File>, src_fd: i32) -> Result<File, ExecError> {
    // Check active redirects first (FDs opened earlier in this redirect list).
    if let Some(file) = active.stdout.as_ref().filter(|_| src_fd == 1) {
        return file.try_clone().map_err(ExecError::Io);
    }
    if let Some(file) = active.stderr.as_ref().filter(|_| src_fd == 2) {
        return file.try_clone().map_err(ExecError::Io);
    }
    if let Some(file) = active.stdin.as_ref().filter(|_| src_fd == 0) {
        return file.try_clone().map_err(ExecError::Io);
    }
    if let Some(file) = active.extra_fds.get(&src_fd) {
        return file.try_clone().map_err(ExecError::Io);
    }
    // Check persistent fd_table.
    if let Some(file) = fd_table.get(&src_fd) {
        return file.try_clone().map_err(ExecError::Io);
    }
    // For FDs 0-2, fall back to duplicating the process's own file descriptors.
    if let Some(file) = dup_process_fd(src_fd) {
        return Ok(file);
    }
    Err(ExecError::BadRedirect(format!("{}: bad file descriptor", src_fd)))
}

/// Clone a file descriptor for use as a read source.
fn clone_fd_for_read(active: &ActiveRedirects, fd_table: &HashMap<i32, File>, src_fd: i32) -> Result<File, ExecError> {
    // Same resolution order as clone_fd_for_write.
    clone_fd_for_write(active, fd_table, src_fd)
}

/// Duplicate a process-level file descriptor.
///
/// On Unix, attempts `dup(fd)` for any non-negative FD. Returns `None` if
/// the FD doesn't exist (EBADF). This handles both standard streams (0-2)
/// and FDs inherited from parent processes (e.g., subshells inheriting
/// FDs opened by `exec 3>file`).
///
/// On Windows, FDs 0-2 use `GetStdHandle` + `DuplicateHandle`; FDs 3+ use
/// `_get_osfhandle` to convert CRT FD numbers to OS handles.
pub fn dup_process_fd(fd: i32) -> Option<File> {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        if fd < 0 {
            return None;
        }
        // SAFETY: dup() is safe for any non-negative fd; returns -1 on invalid fd.
        let new_fd = unsafe { nix::libc::dup(fd) };
        if new_fd < 0 {
            return None;
        }
        // SAFETY: new_fd is a valid open file descriptor (dup succeeded).
        Some(unsafe { File::from_raw_fd(new_fd) })
    }
    #[cfg(windows)]
    {
        match fd {
            0 => dup_std_handle(windows::Win32::System::Console::STD_INPUT_HANDLE),
            1 => dup_std_handle(windows::Win32::System::Console::STD_OUTPUT_HANDLE),
            2 => dup_std_handle(windows::Win32::System::Console::STD_ERROR_HANDLE),
            _ => dup_crt_fd(fd),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

#[cfg(windows)]
fn dup_std_handle(which: windows::Win32::System::Console::STD_HANDLE) -> Option<File> {
    use std::os::windows::io::FromRawHandle;
    use windows::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE};
    use windows::Win32::System::Console::GetStdHandle;
    use windows::Win32::System::Threading::GetCurrentProcess;

    let handle = unsafe { GetStdHandle(which).ok()? };
    let process = unsafe { GetCurrentProcess() };
    let mut dup_handle = HANDLE::default();
    unsafe {
        DuplicateHandle(
            process,
            handle,
            process,
            &mut dup_handle,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )
        .ok()?;
    }
    Some(unsafe { File::from_raw_handle(dup_handle.0 as _) })
}

/// Duplicate a CRT file descriptor (3+) on Windows.
///
/// Uses `_get_osfhandle` to get the OS handle, then `DuplicateHandle`.
/// Returns `None` if the CRT FD is invalid.
#[cfg(windows)]
fn dup_crt_fd(fd: i32) -> Option<File> {
    use std::os::windows::io::FromRawHandle;
    use windows::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE};
    use windows::Win32::System::Threading::GetCurrentProcess;

    extern "C" {
        fn _get_osfhandle(fd: i32) -> isize;
    }

    let os_handle = unsafe { _get_osfhandle(fd) };
    // _get_osfhandle returns -1 (INVALID_HANDLE_VALUE) on error.
    if os_handle == -1 {
        return None;
    }

    let handle = HANDLE(os_handle as _);
    let process = unsafe { GetCurrentProcess() };
    let mut dup_handle = HANDLE::default();
    unsafe {
        DuplicateHandle(
            process,
            handle,
            process,
            &mut dup_handle,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )
        .ok()?;
    }
    Some(unsafe { File::from_raw_handle(dup_handle.0 as _) })
}

/// Create a temporary file for heredoc/herestring content.
fn tempfile() -> Result<File, ExecError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("thaum-heredoc-{}-{}", std::process::id(), n,));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(ExecError::Io)?;

    // Best-effort removal of temp file. On Unix, the file remains accessible
    // via the open handle even after unlink. On Windows, this may fail (file
    // is still open), which is fine — the OS cleans up temp files.
    let _ = std::fs::remove_file(&path);

    Ok(file)
}

/// Extension trait to seek a File back to the start.
trait SeekToStart {
    fn seek_to_start(&mut self) -> Result<(), ExecError>;
}

impl SeekToStart for File {
    fn seek_to_start(&mut self) -> Result<(), ExecError> {
        use std::io::Seek;
        self.seek(io::SeekFrom::Start(0)).map_err(ExecError::Io)?;
        Ok(())
    }
}
