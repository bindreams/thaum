//! Cross-platform PTY expect library with VT sequence stripping.
//!
//! Spawns a process in a pseudo-terminal and provides `expect`/`send` methods
//! for driving interactive sessions. VT escape sequences are stripped before
//! pattern matching, which is essential on Windows where ConPTY interleaves
//! cursor-movement sequences into output (e.g. a space becomes `\x1b[1C`).

use std::io;
use std::process::Command;
use std::time::{Duration, Instant};

use regex::Regex;

// VT stripping ========================================================================================================

/// Strip ANSI/VT escape sequences from a string.
///
/// Removes:
/// - CSI sequences: `ESC [` ... final byte (`@`–`~`)
/// - OSC sequences: `ESC ]` ... `BEL` (`\x07`) or `ESC \`
/// - Two-byte escapes: `ESC` + single character
pub fn strip_vt(s: &str) -> String {
    let re = Regex::new(concat!(
        r"\x1b\[[^@-~]*[@-~]",  // CSI sequence
        r"|\x1b\][^\x07]*\x07", // OSC sequence (BEL-terminated)
        r"|\x1b\].*?\x1b\\",    // OSC sequence (ST-terminated)
        r"|\x1b[^\[\]]",        // Two-byte escape (ESC + non-bracket char)
    ))
    .expect("VT stripping regex");

    re.replace_all(s, "").into_owned()
}

// PtySession ==========================================================================================================

/// A match returned by [`PtySession::expect`].
#[derive(Debug)]
pub struct Match {
    /// Raw bytes read from the PTY up to and including the match.
    pub raw: Vec<u8>,
    /// VT-stripped text that was searched.
    pub cleaned: String,
    /// Byte offset in `cleaned` where the pattern was found.
    pub offset: usize,
}

/// Error from [`PtySession::expect`].
#[derive(Debug)]
pub enum ExpectError {
    /// Timed out waiting for the pattern.
    Timeout,
    /// PTY closed before the pattern appeared.
    Eof,
    /// I/O error.
    Io(io::Error),
}

/// Interactive PTY session.
pub struct PtySession {
    inner: PtyInner,
    timeout: Option<Duration>,
    buffer: Vec<u8>,
}

impl PtySession {
    /// Spawn a command in a pseudo-terminal.
    pub fn spawn(cmd: Command) -> io::Result<Self> {
        let inner = PtyInner::spawn(cmd)?;
        Ok(PtySession {
            inner,
            timeout: None,
            buffer: Vec::new(),
        })
    }

    /// Set the timeout for `expect` calls.
    pub fn set_expect_timeout(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout;
    }

    /// Wait for `pattern` to appear in the VT-stripped output.
    pub fn expect(&mut self, pattern: &str) -> Result<Match, ExpectError> {
        let deadline = self.timeout.map(|d| Instant::now() + d);

        loop {
            // Check if the pattern already exists in buffered output.
            let cleaned = strip_vt(&String::from_utf8_lossy(&self.buffer));
            if let Some(offset) = cleaned.find(pattern) {
                let raw = self.buffer.clone();
                self.buffer.clear();
                return Ok(Match { raw, cleaned, offset });
            }

            // Check timeout.
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    return Err(ExpectError::Timeout);
                }
            }

            // Read more data from the PTY.
            let remaining = deadline.map(|d| d.saturating_duration_since(Instant::now()));
            match self.inner.read_with_timeout(&mut self.buffer, remaining) {
                Ok(0) => return Err(ExpectError::Eof),
                Ok(_) => {} // Data appended to self.buffer
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    return Err(ExpectError::Timeout);
                }
                Err(e) => return Err(ExpectError::Io(e)),
            }
        }
    }

    /// Send a string to the PTY.
    pub fn send(&mut self, text: &str) -> io::Result<()> {
        self.inner.write_all(text.as_bytes())
    }

    /// Send a string followed by a newline to the PTY.
    pub fn send_line(&mut self, text: &str) -> io::Result<()> {
        self.inner.write_all(text.as_bytes())?;
        self.inner.write_all(b"\r\n")?;
        self.inner.flush()
    }
}

// Platform backend ====================================================================================================

#[cfg(unix)]
mod platform {
    use std::io::{self, Read, Write};
    use std::process::Command;
    use std::time::Duration;

    pub struct PtyInner {
        process: ptyprocess::PtyProcess,
    }

    impl PtyInner {
        pub fn spawn(cmd: Command) -> io::Result<Self> {
            let process = ptyprocess::PtyProcess::spawn(cmd).map_err(io::Error::other)?;
            Ok(PtyInner { process })
        }

        /// Read data from the PTY, appending to `buf`. Returns bytes read.
        pub fn read_with_timeout(&mut self, buf: &mut Vec<u8>, timeout: Option<Duration>) -> io::Result<usize> {
            use std::os::fd::AsRawFd;

            let fd = self.process.get_raw_handle().map_err(io::Error::other)?.as_raw_fd();
            let timeout_ms = timeout.map(|d| d.as_millis() as i32).unwrap_or(-1);

            let mut pollfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };

            let ret = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
            if ret == 0 {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "poll timed out"));
            }
            if ret < 0 {
                return Err(io::Error::last_os_error());
            }

            let mut tmp = [0u8; 4096];
            let n = self
                .process
                .get_raw_handle()
                .map_err(io::Error::other)?
                .read(&mut tmp)?;
            buf.extend_from_slice(&tmp[..n]);
            Ok(n)
        }

        pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
            self.process.get_raw_handle().map_err(io::Error::other)?.write_all(data)
        }

        pub fn flush(&mut self) -> io::Result<()> {
            self.process.get_raw_handle().map_err(io::Error::other)?.flush()
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::io::{self, Read, Write};
    use std::process::Command;
    use std::time::Duration;

    pub struct PtyInner {
        process: conpty::Process,
        output: conpty::io::PipeReader,
        input: conpty::io::PipeWriter,
    }

    impl PtyInner {
        pub fn spawn(cmd: Command) -> io::Result<Self> {
            let mut process = conpty::Process::spawn(cmd).map_err(io::Error::other)?;
            let output = process.output().map_err(io::Error::other)?;
            let input = process.input().map_err(io::Error::other)?;
            Ok(PtyInner { process, output, input })
        }

        /// Read data from the PTY, appending to `buf`. Returns bytes read.
        pub fn read_with_timeout(&mut self, buf: &mut Vec<u8>, timeout: Option<Duration>) -> io::Result<usize> {
            self.output.blocking(false);

            let deadline = timeout.map(|d| std::time::Instant::now() + d);
            let mut tmp = [0u8; 4096];

            loop {
                match self.output.read(&mut tmp) {
                    Ok(0) => {
                        if !self.process.is_alive() {
                            self.output.blocking(true);
                            return Ok(0); // EOF
                        }
                        if let Some(dl) = deadline {
                            if std::time::Instant::now() >= dl {
                                self.output.blocking(true);
                                return Err(io::Error::new(io::ErrorKind::TimedOut, "read timed out"));
                            }
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Ok(n) => {
                        self.output.blocking(true);
                        buf.extend_from_slice(&tmp[..n]);
                        return Ok(n);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        if let Some(dl) = deadline {
                            if std::time::Instant::now() >= dl {
                                self.output.blocking(true);
                                return Err(io::Error::new(io::ErrorKind::TimedOut, "read timed out"));
                            }
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => {
                        self.output.blocking(true);
                        return Err(e);
                    }
                }
            }
        }

        pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
            self.input.write_all(data)
        }

        pub fn flush(&mut self) -> io::Result<()> {
            self.input.flush()
        }
    }
}

use platform::PtyInner;

#[cfg(test)]
fn main() {
    skuld::run_all();
}
