//! Uniform fd-table I/O context for shell execution.
//!
//! `IoContext` holds a `HashMap<i32, File>` — all file descriptors are treated
//! uniformly regardless of number. TTY-ness is queried on-demand via
//! `is_file_terminal()`, not cached. `IoContext::from_process()` creates an
//! IoContext from dup'd process handles; `CapturedIo` uses OS pipes for test
//! capture.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Write};
use std::thread::JoinHandle;

use crate::exec::buffered_file::dup_process_fd;
use crate::exec::platform::is_file_terminal;

/// I/O context for shell execution — a uniform POSIX-style fd table.
///
/// All file descriptors (0, 1, 2, 3+) are stored in a single `HashMap<i32, File>`.
/// Direction (read vs write) is a runtime property of the underlying OS handle,
/// not a type-level constraint. TTY-ness is queried on-demand.
pub struct IoContext {
    fds: HashMap<i32, File>,
    /// Fds the script explicitly closed with `N>&-` / `N<&-`.
    ///
    /// Distinct from simply being absent from `fds`. An absent fd is one the
    /// shell never touched, which it may still resolve against the host
    /// process's real fd table — that is ordinary POSIX inheritance and bash
    /// does the same. A *closed* fd must not be resolvable that way, and must
    /// not be inherited by children.
    ///
    /// thaum never calls `close(2)` on the host's descriptor: it is not ours to
    /// close. Recording the close and refusing to hand the number out is
    /// observably equivalent for the script and leaves the embedder intact.
    closed: HashSet<i32>,
    /// Fds that should report as terminals regardless of the underlying handle.
    /// Used in tests to exercise the PTY code path with pipe-backed fds.
    tty_overrides: HashSet<i32>,
}

impl IoContext {
    /// Create an IoContext from a pre-built fd table.
    pub fn new(fds: HashMap<i32, File>) -> Self {
        IoContext {
            fds,
            closed: HashSet::new(),
            tty_overrides: HashSet::new(),
        }
    }

    /// Get an immutable reference to a file descriptor.
    pub fn fd(&self, fd: i32) -> Option<&File> {
        self.fds.get(&fd)
    }

    /// Get a mutable reference to a file descriptor.
    pub fn fd_mut(&mut self, fd: i32) -> Option<&mut File> {
        self.fds.get_mut(&fd)
    }

    /// Insert or replace a file descriptor.
    ///
    /// Reopening a number clears any closed marker: `exec 3>&-; exec 3>file`
    /// leaves fd 3 usable again, in thaum as in bash.
    pub fn set_fd(&mut self, fd: i32, file: File) {
        self.closed.remove(&fd);
        self.fds.insert(fd, file);
    }

    /// Remove a file descriptor, returning it if present.
    ///
    /// Bookkeeping only — this does **not** mark the fd closed. Use
    /// [`close_fd`](IoContext::close_fd) for `N>&-`.
    pub fn remove_fd(&mut self, fd: i32) -> Option<File> {
        self.fds.remove(&fd)
    }

    /// Mark a file descriptor as explicitly closed by the script.
    ///
    /// Drops any handle the shell held and records the number so it is neither
    /// resolvable by a later `>&N` nor inherited by a spawned child.
    pub fn close_fd(&mut self, fd: i32) {
        self.fds.remove(&fd);
        self.closed.insert(fd);
    }

    /// Whether the script explicitly closed this descriptor.
    pub fn is_closed(&self, fd: i32) -> bool {
        self.closed.contains(&fd)
    }

    /// Every descriptor the script explicitly closed.
    pub fn closed_fds(&self) -> &HashSet<i32> {
        &self.closed
    }

    /// Set or clear the closed marker directly. For redirect save/restore only.
    pub fn set_closed_marker(&mut self, fd: i32, closed: bool) {
        if closed {
            self.closed.insert(fd);
        } else {
            self.closed.remove(&fd);
        }
    }

    /// Clone (dup) a file descriptor's OS handle.
    pub fn try_clone_fd(&self, fd: i32) -> io::Result<File> {
        self.fds
            .get(&fd)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("fd {fd} not open")))
            .and_then(|f| f.try_clone())
    }

    /// Access the full fd table.
    pub fn fds(&self) -> &HashMap<i32, File> {
        &self.fds
    }

    /// Check whether a file descriptor refers to a terminal.
    ///
    /// Checks tty overrides first (for test PTY simulation), then queries the
    /// actual OS handle via `is_file_terminal()`.
    pub fn is_tty(&self, fd: i32) -> bool {
        if self.tty_overrides.contains(&fd) {
            return true;
        }
        self.fds.get(&fd).is_some_and(is_file_terminal)
    }

    /// Mark a fd as reporting terminal-ness regardless of the underlying handle.
    ///
    /// Used in tests to exercise PTY code paths with pipe-backed fds.
    pub fn set_tty_override(&mut self, fd: i32) {
        self.tty_overrides.insert(fd);
    }

    /// Check whether a fd has a tty override set.
    pub fn has_tty_override(&self, fd: i32) -> bool {
        self.tty_overrides.contains(&fd)
    }

    /// Remove a tty override for a fd.
    ///
    /// Used by redirect apply/restore to clear overrides for redirected fds,
    /// preventing stale TTY status after a file redirect replaces the handle.
    pub fn clear_tty_override(&mut self, fd: i32) {
        self.tty_overrides.remove(&fd);
    }

    /// Save the current fd and replace it. Returns the saved fd (or `None` if
    /// the fd didn't exist before).
    ///
    /// Used by redirect apply/restore: save the original, set the redirect,
    /// then later restore via [`restore()`](IoContext::restore).
    pub fn save_and_set(&mut self, fd: i32, file: File) -> Option<File> {
        self.closed.remove(&fd);
        self.fds.insert(fd, file)
    }

    /// Restore a previously saved fd.
    ///
    /// - `saved = Some(file)`: put back the original fd.
    /// - `saved = None`: the fd was newly added by a redirect — remove it.
    pub fn restore(&mut self, fd: i32, saved: Option<File>) {
        self.closed.remove(&fd);
        match saved {
            Some(file) => {
                self.fds.insert(fd, file);
            }
            None => {
                self.fds.remove(&fd);
            }
        }
    }
}

/// Opens the platform null device (`/dev/null` on Unix, `NUL` on Windows).
///
/// Opened in read+write mode so the handle works for any fd direction.
/// Panics if the null device cannot be opened (catastrophic system error).
pub(super) fn open_null_device() -> File {
    #[cfg(unix)]
    {
        File::options()
            .read(true)
            .write(true)
            .open("/dev/null")
            .expect("failed to open /dev/null")
    }
    #[cfg(windows)]
    {
        File::options()
            .read(true)
            .write(true)
            .open("NUL")
            .expect("failed to open NUL")
    }
    #[cfg(not(any(unix, windows)))]
    {
        compile_error!("unsupported platform: no null device")
    }
}

impl IoContext {
    /// Create an `IoContext` by duplicating the process's standard file descriptors.
    ///
    /// Panics if fds 0, 1, or 2 are not available (closed).
    pub fn from_process() -> IoContext {
        IoContext::new(HashMap::from([
            (0, dup_process_fd(0).expect("fd 0 (stdin) not available")),
            (1, dup_process_fd(1).expect("fd 1 (stdout) not available")),
            (2, dup_process_fd(2).expect("fd 2 (stderr) not available")),
        ]))
    }
}

// CapturedIo ==========================================================================================================

/// Test capture using OS pipes with eager drain threads.
///
/// Background threads continuously read from pipe read-ends while the executor
/// writes to the write-ends in IoContext. This prevents deadlock when output
/// exceeds the OS pipe buffer size (~64KB). Call `finish()` after execution to
/// close the write-ends (via IoContext drop) and collect the captured output.
pub struct CapturedIo {
    stdout_thread: JoinHandle<io::Result<Vec<u8>>>,
    stderr_thread: JoinHandle<io::Result<Vec<u8>>>,
}

impl CapturedIo {
    /// Create an IoContext with pipe-backed fds 0/1/2 and a capture handle.
    ///
    /// Fd 0 (stdin) is backed by an empty pipe (reads return EOF immediately).
    /// Fds 1/2 are pipe write-ends; background threads drain the read-ends
    /// concurrently so writes never block on a full pipe buffer.
    pub fn new() -> (IoContext, Self) {
        let (stdout_read, stdout_write) = crate::exec::pipeline::os_pipe().expect("failed to create stdout pipe");
        let (stderr_read, stderr_write) = crate::exec::pipeline::os_pipe().expect("failed to create stderr pipe");
        let (stdin_read, _stdin_write) = crate::exec::pipeline::os_pipe().expect("failed to create stdin pipe");

        let io = IoContext::new(HashMap::from([(0, stdin_read), (1, stdout_write), (2, stderr_write)]));

        let capture = Self::spawn_drains(stdout_read, stderr_read);
        (io, capture)
    }

    /// Create an IoContext with pre-loaded stdin data.
    ///
    /// The data is written to the pipe via a background thread, so there is no
    /// size limitation (arbitrarily large data is handled without deadlock).
    pub fn with_stdin(data: &[u8]) -> (IoContext, Self) {
        let (stdout_read, stdout_write) = crate::exec::pipeline::os_pipe().expect("failed to create stdout pipe");
        let (stderr_read, stderr_write) = crate::exec::pipeline::os_pipe().expect("failed to create stderr pipe");
        let (stdin_read, stdin_write) = crate::exec::pipeline::os_pipe().expect("failed to create stdin pipe");

        // Spawn a writer thread to avoid deadlock when data exceeds the OS pipe buffer.
        // The thread closes the write-end on completion so reads see EOF after the data.
        let data = data.to_vec();
        std::thread::spawn(move || {
            let mut w = stdin_write;
            let _ = w.write_all(&data);
        });

        let io = IoContext::new(HashMap::from([(0, stdin_read), (1, stdout_write), (2, stderr_write)]));

        let capture = Self::spawn_drains(stdout_read, stderr_read);
        (io, capture)
    }

    /// Spawn background threads to drain pipe read-ends into `Vec<u8>`.
    fn spawn_drains(mut stdout_read: File, mut stderr_read: File) -> Self {
        use std::io::Read;
        let stdout_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            stdout_read.read_to_end(&mut buf)?;
            Ok(buf)
        });
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            stderr_read.read_to_end(&mut buf)?;
            Ok(buf)
        });
        CapturedIo {
            stdout_thread,
            stderr_thread,
        }
    }

    /// Close the write-ends (by dropping IoContext) and collect captured output.
    ///
    /// The IoContext must be passed back so its fds 1/2 (write-ends) are closed,
    /// triggering EOF on the drain threads.
    pub fn finish(self, io: IoContext) -> CapturedOutput {
        // Drop IoContext first — closes write-ends → EOF on drain threads.
        drop(io);

        let stdout = self
            .stdout_thread
            .join()
            .expect("stdout drain thread panicked")
            .unwrap_or_default();
        let stderr = self
            .stderr_thread
            .join()
            .expect("stderr drain thread panicked")
            .unwrap_or_default();

        CapturedOutput { stdout, stderr }
    }
}

/// Captured output from a `CapturedIo` session.
pub struct CapturedOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl CapturedOutput {
    /// Return captured stdout as a string (lossy UTF-8 conversion).
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Return captured stderr as a string (lossy UTF-8 conversion).
    pub fn stderr_string(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    /// Raw stdout bytes.
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout
    }

    /// Raw stderr bytes.
    pub fn stderr_bytes(&self) -> &[u8] {
        &self.stderr
    }
}

#[cfg(test)]
#[path = "io_context_tests.rs"]
mod tests;
