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
    #[cfg(windows)]
    {
        win::file_owned_by_current_user(path)
    }
    #[cfg(not(any(unix, windows)))]
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
    #[cfg(windows)]
    {
        win::file_group_matches_current_user(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        false
    }
}

/// Check whether two paths refer to the same file (same device + inode).
pub fn files_are_same(path_a: &str, path_b: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let (Ok(a), Ok(b)) = (std::fs::metadata(path_a), std::fs::metadata(path_b)) else {
            return false;
        };
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(windows)]
    {
        win::files_are_same(path_a, path_b)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path_a, path_b);
        false
    }
}

/// Check whether a path is a named pipe (FIFO).
pub fn is_named_pipe(path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        std::fs::metadata(path)
            .map(|m| m.file_type().is_fifo())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        win::is_named_pipe(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        false
    }
}

/// Get the current user's numeric ID.
/// On Windows, returns the last sub-authority (RID) of the user's SID.
#[cfg(not(unix))]
pub fn current_uid() -> u32 {
    #[cfg(windows)]
    {
        win::current_user_rid().unwrap_or(0)
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Get the current user's effective UID. On Windows, returns 0 if elevated
/// (administrator), otherwise the user's RID.
#[cfg(not(unix))]
pub fn current_euid() -> u32 {
    #[cfg(windows)]
    {
        if win::is_elevated() {
            0
        } else {
            win::current_user_rid().unwrap_or(0)
        }
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Get the current user's group IDs. On Windows, returns the RIDs of the
/// user's token groups.
#[cfg(not(unix))]
pub fn current_groups() -> Vec<u32> {
    #[cfg(windows)]
    {
        win::current_group_rids()
    }
    #[cfg(not(windows))]
    {
        vec![0]
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

#[cfg(windows)]
#[path = "platform/windows.rs"]
mod win;

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
