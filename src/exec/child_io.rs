//! Concurrent child process pipe draining.
//!
//! When both stdout and stderr are piped from a child process, reading them
//! sequentially can deadlock: the child may fill the stderr pipe buffer while
//! the parent blocks on stdout (or vice versa). This module provides
//! `drain_child_pipes()` which reads both pipes concurrently via a scoped
//! thread, avoiding the circular wait.

use std::io::{Read, Write};

use crate::exec::command_ex::ChildEx;
use crate::exec::error::ExecError;
use crate::exec::io_context::IoContext;

/// Clean ConPTY output: strip VT escape sequences and `\r`.
///
/// ConPTY is a full terminal emulator. Its output stream contains initialization
/// sequences (clear screen, cursor hide), window title sets, and `\r\n` line
/// endings — none of which belong in the relayed output. This function removes:
///
/// - **CSI sequences**: `ESC [` ... final byte (`@`–`~`)
/// - **OSC sequences**: `ESC ]` ... `BEL` (`\x07`) or `ESC \` (ST)
/// - **Two-byte escapes**: `ESC` + single non-`[`/`]` character
/// - **Carriage returns** (`\r`): normalizes `\r\n` → `\n`
///
/// Everything else is preserved, including tabs, newlines, and high bytes.
fn clean_conpty_output(buf: &mut Vec<u8>) {
    let mut w = 0; // write cursor
    let mut r = 0; // read cursor
    while r < buf.len() {
        match buf[r] {
            b'\x1b' => {
                r += 1;
                if r >= buf.len() {
                    break;
                }
                match buf[r] {
                    b'[' => {
                        // CSI: skip until final byte in @–~ (0x40–0x7E).
                        r += 1;
                        while r < buf.len() && !(0x40..=0x7E).contains(&buf[r]) {
                            r += 1;
                        }
                        if r < buf.len() {
                            r += 1; // skip final byte
                        }
                    }
                    b']' => {
                        // OSC: skip until BEL (\x07) or ST (ESC \).
                        r += 1;
                        while r < buf.len() {
                            if buf[r] == b'\x07' {
                                r += 1;
                                break;
                            }
                            if buf[r] == b'\x1b' && r + 1 < buf.len() && buf[r + 1] == b'\\' {
                                r += 2;
                                break;
                            }
                            r += 1;
                        }
                    }
                    _ => {
                        // Two-byte escape: ESC + one character. Skip both.
                        r += 1;
                    }
                }
            }
            b'\r' => {
                r += 1;
            }
            _ => {
                buf[w] = buf[r];
                w += 1;
                r += 1;
            }
        }
    }
    buf.truncate(w);
}

/// Read piped stdout and stderr from a child process concurrently.
///
/// Takes ownership of the stdout (fd 1) and stderr (fd 2) pipes from `child`.
/// Stdout is drained on the current thread; stderr is drained on a scoped
/// background thread. Returns `(stdout_bytes, stderr_bytes)`.
///
/// If the child has no pipe for a given fd, the corresponding buffer is empty.
pub(super) fn drain_child_pipes(child: &mut ChildEx) -> Result<(Vec<u8>, Vec<u8>), ExecError> {
    let stdout_pipe = child.take_pipe(1);
    let stderr_pipe = child.take_pipe(2);

    std::thread::scope(|s| {
        // Drain stderr on a background thread.
        let stderr_thread = stderr_pipe.map(|mut pipe| {
            s.spawn(move || {
                let mut buf = Vec::new();
                pipe.read_to_end(&mut buf).map(|_| buf)
            })
        });

        // Drain stdout on the current thread.
        let mut stdout_buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            pipe.read_to_end(&mut stdout_buf).map_err(ExecError::Io)?;
        }

        // Join stderr thread.
        let mut stderr_buf = Vec::new();
        if let Some(handle) = stderr_thread {
            stderr_buf = handle
                .join()
                .map_err(|_| ExecError::Io(std::io::Error::other("stderr reader thread panicked")))?
                .map_err(ExecError::Io)?;
        }

        Ok((stdout_buf, stderr_buf))
    })
}

/// Drain pipes, wait, and mark a ConPTY child completed.
///
/// ConPTY output pipes don't EOF until the pseudo console is closed. This
/// function runs pipe draining and process wait concurrently: a background
/// thread waits for the child and closes the ConPTY (triggering EOF), while
/// the main thread drains the pipes.
///
/// Always calls `mark_completed` before returning — even on error — so that
/// subsequent `wait()` calls (e.g. from the pipeline orchestrator) hit the
/// `Completed` path instead of waiting on already-closed handles.
pub(super) fn drain_and_wait_conpty(child: &mut ChildEx) -> Result<(i32, Vec<u8>, Vec<u8>), ExecError> {
    let (stdout_pipe, stderr_pipe, wait_fn) = child.take_pipes_and_waiter();

    let result = std::thread::scope(|s| {
        // Wait for the child on a background thread. When the child exits,
        // wait() closes the ConPTY, which triggers EOF on the output pipes.
        let wait_thread = s.spawn(wait_fn);

        // Drain stderr on another background thread.
        let stderr_thread = stderr_pipe.map(|mut pipe| {
            s.spawn(move || {
                let mut buf = Vec::new();
                pipe.read_to_end(&mut buf).map(|_| buf)
            })
        });

        // Drain stdout on the current thread.
        let mut stdout_buf = Vec::new();
        let stdout_err = if let Some(mut pipe) = stdout_pipe {
            pipe.read_to_end(&mut stdout_buf).err()
        } else {
            None
        };

        // Join stderr thread.
        let mut stderr_buf = Vec::new();
        let stderr_err = if let Some(handle) = stderr_thread {
            match handle.join() {
                Ok(Ok(buf)) => {
                    stderr_buf = buf;
                    None
                }
                Ok(Err(e)) => Some(ExecError::Io(e)),
                Err(_) => Some(ExecError::Io(std::io::Error::other("stderr reader thread panicked"))),
            }
        } else {
            None
        };

        // Always join wait thread — the process handles are already closed
        // inside it, so we must retrieve the status regardless of pipe errors.
        let wait_result = wait_thread
            .join()
            .map_err(|_| ExecError::Io(std::io::Error::other("wait thread panicked")))
            .and_then(|r| r.map_err(ExecError::Io));

        // Propagate errors in priority order: wait > stdout > stderr.
        let status = match wait_result {
            Ok(s) => s,
            Err(e) => return Err(e),
        };
        if let Some(e) = stdout_err {
            return Err(ExecError::Io(e));
        }
        if let Some(e) = stderr_err {
            return Err(e);
        }

        Ok((status, stdout_buf, stderr_buf))
    });

    // Always mark completed so subsequent wait() calls don't use closed handles.
    let status = match &result {
        Ok((s, _, _)) => *s,
        Err(_) => 1, // Fallback exit code on error.
    };
    child.mark_completed(status);

    result
}

/// Drain a child's stdout/stderr pipes, relay through `io`, and wait.
///
/// For ConPTY children, drain and wait run concurrently (pipes don't EOF until
/// the pseudo console is closed). For regular children, drains pipes first,
/// then waits.
pub(super) fn drain_and_relay(child: &mut ChildEx, io: &mut IoContext) -> Result<i32, ExecError> {
    // Read conpty_output_fd before drain_and_wait_conpty, which calls
    // mark_completed and changes the inner variant to Completed.
    let conpty_fd = child.conpty_output_fd();
    let (status, mut stdout_buf, mut stderr_buf) = if child.has_conpty() {
        drain_and_wait_conpty(child)?
    } else {
        let (out, err) = drain_child_pipes(child)?;
        let status = child.wait().map_err(ExecError::Io)?;
        (status, out, err)
    };
    // Clean the buffer that carries ConPTY output (fd 1 or fd 2).
    match conpty_fd {
        Some(1) => clean_conpty_output(&mut stdout_buf),
        Some(2) => clean_conpty_output(&mut stderr_buf),
        _ => {}
    }
    if !stdout_buf.is_empty() {
        if let Some(stdout) = io.fd_mut(1) {
            stdout.write_all(&stdout_buf).map_err(ExecError::Io)?;
        }
    }
    if !stderr_buf.is_empty() {
        if let Some(stderr) = io.fd_mut(2) {
            stderr.write_all(&stderr_buf).map_err(ExecError::Io)?;
        }
    }
    Ok(status)
}

/// Drain a child's stdout/stderr pipes and relay through `io` without waiting.
///
/// For pipeline stages where the orchestrator handles the wait separately.
/// ConPTY children are an exception: they require a concurrent wait to trigger
/// EOF on their output pipes, so this function waits and marks them completed.
pub(super) fn drain_to_io(child: &mut ChildEx, io: &mut IoContext) -> Result<(), ExecError> {
    let conpty_fd = child.conpty_output_fd();
    let (mut stdout_buf, mut stderr_buf) = if child.has_conpty() {
        let (_status, out, err) = drain_and_wait_conpty(child)?;
        (out, err)
    } else {
        drain_child_pipes(child)?
    };
    match conpty_fd {
        Some(1) => clean_conpty_output(&mut stdout_buf),
        Some(2) => clean_conpty_output(&mut stderr_buf),
        _ => {}
    }
    if !stdout_buf.is_empty() {
        if let Some(stdout) = io.fd_mut(1) {
            stdout.write_all(&stdout_buf).map_err(ExecError::Io)?;
        }
    }
    if !stderr_buf.is_empty() {
        if let Some(stderr) = io.fd_mut(2) {
            stderr.write_all(&stderr_buf).map_err(ExecError::Io)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::clean_conpty_output;

    skuld::default_labels!(exec);

    #[skuld::test]
    fn clean_replaces_crlf_with_lf() {
        let mut buf = b"hello\r\nworld\r\n".to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, b"hello\nworld\n");
    }

    #[skuld::test]
    fn clean_strips_standalone_cr() {
        // strip_ansi_escapes treats standalone \r as a control character and
        // removes it. This is fine — ConPTY output doesn't contain meaningful
        // standalone \r; all carriage returns are part of \r\n pairs.
        let mut buf = b"hello\rworld\r\n".to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, b"helloworld\n");
    }

    #[skuld::test]
    fn clean_empty() {
        let mut buf = Vec::new();
        clean_conpty_output(&mut buf);
        assert!(buf.is_empty());
    }

    #[skuld::test]
    fn clean_no_escapes() {
        let mut buf = b"hello\nworld\n".to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, b"hello\nworld\n");
    }

    #[skuld::test]
    fn clean_strips_vt_sequences() {
        // Simulate ConPTY output with CSI sequences and OSC title set.
        let mut buf = b"\x1b[2J\x1b[Hhello\r\n\x1b]0;title\x07world\r\n".to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, b"hello\nworld\n");
    }

    #[skuld::test]
    fn clean_preserves_tabs() {
        let mut buf = b"col1\tcol2\tcol3\n".to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, b"col1\tcol2\tcol3\n");
    }

    #[skuld::test]
    fn clean_preserves_high_bytes() {
        // UTF-8 multibyte: "café\n"
        let mut buf = "café\n".as_bytes().to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, "café\n".as_bytes());
    }

    #[skuld::test]
    fn clean_strips_csi_but_keeps_tabs() {
        let mut buf = b"\x1b[2Jhello\tworld\r\n".to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, b"hello\tworld\n");
    }

    #[skuld::test]
    fn clean_strips_osc_with_st_terminator() {
        // OSC terminated by ST (ESC \) instead of BEL.
        let mut buf = b"\x1b]0;title\x1b\\hello\n".to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, b"hello\n");
    }

    #[skuld::test]
    fn clean_strips_two_byte_escape() {
        // ESC = (keypad application mode)
        let mut buf = b"\x1b=hello\n".to_vec();
        clean_conpty_output(&mut buf);
        assert_eq!(buf, b"hello\n");
    }
}
