//! Pluggable I/O context for stdin/stdout/stderr. `ProcessIo` wraps the real
//! process streams; `CapturedIo` uses in-memory buffers for testing.

use std::io::{self, Cursor, Read, Write};

/// I/O context for shell execution.
///
/// Holds references to stdin/stdout/stderr streams. For live execution, these
/// point to the process streams. For testing, they point to in-memory buffers.
pub struct IoContext<'io> {
    pub stdin: &'io mut dyn Read,
    pub stdout: &'io mut dyn Write,
    pub stderr: &'io mut dyn Write,
    /// When true, external commands pipe their output through this IoContext
    /// for capturing. When false (live mode), external commands inherit the
    /// parent process's stdout/stderr handles directly.
    pub capturing: bool,
}

impl<'io> IoContext<'io> {
    /// Create an I/O context from arbitrary Read/Write implementations.
    pub fn new(
        stdin: &'io mut dyn Read,
        stdout: &'io mut dyn Write,
        stderr: &'io mut dyn Write,
        capturing: bool,
    ) -> Self {
        IoContext {
            stdin,
            stdout,
            stderr,
            capturing,
        }
    }
}

/// I/O context backed by the process stdin/stdout/stderr.
pub struct ProcessIo {
    stdin: io::Stdin,
    stdout: io::Stdout,
    stderr: io::Stderr,
}

impl ProcessIo {
    /// Capture the real process stdin/stdout/stderr handles.
    pub fn new() -> Self {
        ProcessIo {
            stdin: io::stdin(),
            stdout: io::stdout(),
            stderr: io::stderr(),
        }
    }

    /// Borrow the process streams as an `IoContext` for executor use.
    ///
    /// The returned context has `capturing: false`, so external commands
    /// inherit parent stdout/stderr handles directly.
    pub fn context(&mut self) -> IoContext<'_> {
        IoContext {
            stdin: &mut self.stdin,
            stdout: &mut self.stdout,
            stderr: &mut self.stderr,
            capturing: false,
        }
    }
}

impl Default for ProcessIo {
    fn default() -> Self {
        Self::new()
    }
}

/// I/O context backed by in-memory buffers for testing.
pub struct CapturedIo {
    pub stdin: Cursor<Vec<u8>>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Default for CapturedIo {
    fn default() -> Self {
        Self::new()
    }
}

impl CapturedIo {
    /// Create empty capture buffers (stdin is empty, stdout/stderr start empty).
    pub fn new() -> Self {
        CapturedIo {
            stdin: Cursor::new(Vec::new()),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    /// Create a captured I/O context with pre-loaded stdin data.
    pub fn with_stdin(data: &[u8]) -> Self {
        CapturedIo {
            stdin: Cursor::new(data.to_vec()),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    /// Borrow the capture buffers as an `IoContext` for executor use.
    ///
    /// The returned context has `capturing: true`, so external commands
    /// pipe their output through the capture buffers.
    pub fn context(&mut self) -> IoContext<'_> {
        IoContext {
            stdin: &mut self.stdin,
            stdout: &mut self.stdout,
            stderr: &mut self.stderr,
            capturing: true,
        }
    }

    /// Return captured stdout as a string (lossy UTF-8 conversion).
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Return captured stderr as a string (lossy UTF-8 conversion).
    pub fn stderr_string(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

#[cfg(test)]
#[path = "io_context_tests.rs"]
mod tests;
