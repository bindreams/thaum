//! File handle with optional read-ahead buffering + process fd duplication.

#[cfg(test)]
#[path = "buffered_file_tests.rs"]
mod tests;

use std::fs::File;
use std::io::{self, Read, Write};

/// File handle with a read-ahead buffer.
///
/// Wraps `std::fs::File` with transparent 8 KB buffering for `Read`. External
/// command FD setup calls [`try_clone()`](BufferedFile::try_clone) on the inner
/// File, bypassing the buffer — the OS fd position is shared via `dup()`.
pub(crate) struct BufferedFile {
    file: File,
    buf: Vec<u8>,
    pos: usize,
    /// When true, reads bypass the buffer and go directly to the OS.
    /// Used for cloned FDs (`<&N`/`>&N`) where the OS fd position is
    /// shared between multiple consumers.
    passthrough: bool,
}

impl BufferedFile {
    pub fn new(file: File) -> Self {
        BufferedFile {
            file,
            buf: Vec::new(),
            pos: 0,
            passthrough: false,
        }
    }

    /// Create without read buffering. For cloned FDs where the OS fd
    /// position is shared — buffering would over-read and desync.
    pub fn passthrough(file: File) -> Self {
        BufferedFile {
            file,
            buf: Vec::new(),
            pos: 0,
            passthrough: true,
        }
    }

    /// Clone the underlying OS file descriptor (for child process inheritance).
    pub fn try_clone(&self) -> io::Result<File> {
        self.file.try_clone()
    }

    /// Consume this wrapper and return the inner `File`.
    pub fn into_inner(self) -> File {
        self.file
    }
}

impl Read for BufferedFile {
    fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
        if self.passthrough {
            return self.file.read(dest);
        }
        // Serve from buffer if available.
        if self.pos < self.buf.len() {
            let available = &self.buf[self.pos..];
            let n = dest.len().min(available.len());
            dest[..n].copy_from_slice(&available[..n]);
            self.pos += n;
            return Ok(n);
        }
        // For large reads, bypass the buffer entirely.
        if dest.len() >= 8192 {
            return self.file.read(dest);
        }
        // Refill buffer from file.
        self.buf.resize(8192, 0);
        self.pos = 0;
        let n = self.file.read(&mut self.buf)?;
        if n == 0 {
            self.buf.clear();
            return Ok(0);
        }
        self.buf.truncate(n);
        let to_copy = dest.len().min(n);
        dest[..to_copy].copy_from_slice(&self.buf[..to_copy]);
        self.pos = to_copy;
        Ok(to_copy)
    }
}

impl Write for BufferedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

// dup_process_fd =======================================================================================================

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
        let _ = fd;
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
