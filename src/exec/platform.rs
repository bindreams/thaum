//! Platform-specific queries (file ownership, terminal detection).
//!
//! Each function isolates `#[cfg]` branching so callers don't need to.

/// Check whether a file is owned by the current effective user.
pub fn file_owned_by_current_user(path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .map(|m| m.uid() == nix::unistd::geteuid().as_raw())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// Check whether a file is owned by the current effective group.
pub fn file_owned_by_current_group(path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .map(|m| m.gid() == nix::unistd::getegid().as_raw())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// Check whether a file descriptor is associated with a terminal.
///
/// On Unix, calls `isatty()` via the `nix` crate (safe wrapper).
/// On Windows, converts the CRT file descriptor to a HANDLE via
/// `_get_osfhandle()` and checks with `GetConsoleMode` + MSYS/Cygwin
/// PTY heuristics (via `std::io::IsTerminal`).
pub fn is_fd_terminal(fd: i32) -> bool {
    if fd < 0 {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::fd::BorrowedFd;
        // SAFETY: isatty() handles invalid FDs gracefully (returns EBADF/ENOTTY).
        // The borrow does not outlive this call and we don't close the FD.
        let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
        nix::unistd::isatty(borrowed).unwrap_or(false)
    }
    #[cfg(windows)]
    {
        use std::io::IsTerminal;
        use std::os::windows::io::BorrowedHandle;
        extern "C" {
            fn _isatty(fd: i32) -> i32;
            fn _get_osfhandle(fd: i32) -> isize;
            fn _set_thread_local_invalid_parameter_handler(
                handler: Option<unsafe extern "C" fn(*const u16, *const u16, *const u16, u32, usize)>,
            ) -> Option<unsafe extern "C" fn(*const u16, *const u16, *const u16, u32, usize)>;
        }
        unsafe extern "C" fn noop_handler(_: *const u16, _: *const u16, _: *const u16, _: u32, _: usize) {}

        // Suppress the CRT invalid-parameter handler so out-of-range FDs
        // don't trigger __fastfail (STATUS_STACK_BUFFER_OVERRUN).
        let prev = unsafe { _set_thread_local_invalid_parameter_handler(Some(noop_handler)) };
        let is_tty = unsafe { _isatty(fd) };
        unsafe { _set_thread_local_invalid_parameter_handler(prev) };

        if is_tty == 0 {
            return false;
        }
        // fd is valid and points to a character device. Get the OS handle for
        // the full is_terminal() check (covers MSYS/Cygwin PTYs too).
        let prev = unsafe { _set_thread_local_invalid_parameter_handler(Some(noop_handler)) };
        let handle = unsafe { _get_osfhandle(fd) };
        unsafe { _set_thread_local_invalid_parameter_handler(prev) };

        if handle == -1 || handle == -2 {
            return false;
        }
        // SAFETY: _isatty confirmed the fd is valid, so the handle is valid.
        // The borrow does not outlive this call.
        unsafe { BorrowedHandle::borrow_raw(handle as _) }.is_terminal()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = fd;
        false
    }
}

/// Get the parent process ID on Windows via the toolhelp snapshot API.
#[cfg(windows)]
pub fn get_parent_pid() -> Option<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };

    let pid = std::process::id();

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    let mut entry = PROCESSENTRY32 {
        dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
        ..Default::default()
    };

    let found = unsafe {
        if Process32First(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    break Some(entry.th32ParentProcessID);
                }
                if Process32Next(snapshot, &mut entry).is_err() {
                    break None;
                }
            }
        } else {
            None
        }
    };

    unsafe { CloseHandle(snapshot) }.ok();
    found
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
