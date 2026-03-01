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
        }
        // _isatty handles invalid fds gracefully (returns 0, sets errno to
        // EBADF) without triggering the CRT invalid parameter handler.
        // _get_osfhandle on an out-of-range fd would invoke __fastfail.
        if unsafe { _isatty(fd) } == 0 {
            return false;
        }
        // fd is valid and points to a character device. Get the OS handle for
        // the full is_terminal() check (covers MSYS/Cygwin PTYs too).
        let handle = unsafe { _get_osfhandle(fd) };
        debug_assert!(handle != -1 && handle != -2);
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

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
