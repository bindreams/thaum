//! Concurrent child process pipe draining.
//!
//! When both stdout and stderr are piped from a child process, reading them
//! sequentially can deadlock: the child may fill the stderr pipe buffer while
//! the parent blocks on stdout (or vice versa). This module provides
//! `drain_child_pipes()` which reads both pipes concurrently via a scoped
//! thread, avoiding the circular wait.

use std::io::Read;

use crate::exec::command_ex::ChildEx;
use crate::exec::error::ExecError;

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
