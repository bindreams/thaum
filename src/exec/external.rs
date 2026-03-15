//! External (non-builtin) command execution via fork/exec. Sets up redirections,
//! exported environment variables, and extra FD mappings before spawning.
//!
//! Stdout and stderr are always piped and relayed through `IoContext`, so the
//! caller controls where output goes (real process handles in production,
//! in-memory buffers in tests). When the parent stream is a terminal, a PTY
//! is used instead of a plain pipe so that `isatty()` returns true in the child.

use crate::exec::child_io;
use crate::exec::command_ex::{CommandEx, Fd};
use crate::exec::error::ExecError;
use crate::exec::io_context::IoContext;
use crate::exec::redirect::ActiveRedirects;
use crate::exec::Executor;

impl Executor {
    /// Execute an external command via fork/exec.
    ///
    /// Redirections are pre-resolved in `active`. Stdout and stderr are piped
    /// when not explicitly redirected, and the captured output is relayed
    /// through `io` so that `CapturedIo` (tests) and `ProcessIo` (live) both
    /// receive the child's output.
    pub(super) fn execute_external(
        &mut self,
        name: &str,
        args: &[String],
        assignments: &[crate::ast::Assignment],
        active: &mut ActiveRedirects,
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(1 + args.len());
        argv.push(name.into());
        argv.extend(args.iter().map(std::ffi::OsString::from));

        let mut child_cmd = CommandEx::new(argv);
        child_cmd.cwd = Some(self.env.cwd().to_path_buf());

        // Build environment from exported variables + prefix assignments.
        let mut env: std::collections::HashMap<std::ffi::OsString, std::ffi::OsString> = self
            .env
            .exported_vars()
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        for assignment in assignments {
            let value = self.expand_scalar_assignment(assignment)?;
            env.insert(assignment.name.clone().into(), value.into());
        }
        child_cmd.env = env;

        // Persistent fd_table first (includes FDs 0-2 from `exec` redirects).
        for (&fd, file) in &self.fd_table {
            child_cmd
                .fds
                .insert(fd, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }

        // Per-command redirects override persistent ones: FDs 0-2 from
        // redirect list, then FDs 3+ from extra_fds.
        if let Some(ref file) = active.stdin {
            child_cmd
                .fds
                .insert(0, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }
        if let Some(ref file) = active.stdout {
            child_cmd
                .fds
                .insert(1, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }
        if let Some(ref file) = active.stderr {
            child_cmd
                .fds
                .insert(2, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }
        for (&fd, file) in &active.extra_fds {
            child_cmd
                .fds
                .insert(fd, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }

        // Pipe stdout/stderr through IoContext so the caller controls where
        // output goes. Use a PTY when the parent stream is a terminal so the
        // child sees isatty() == true. `entry().or_insert` respects explicit
        // redirects above.
        child_cmd
            .fds
            .entry(1)
            .or_insert(if io.tty_stdout { Fd::Pty } else { Fd::Pipe });
        child_cmd
            .fds
            .entry(2)
            .or_insert(if io.tty_stderr { Fd::Pty } else { Fd::Pipe });

        match child_cmd.spawn() {
            Ok(mut child) => {
                let status = child_io::drain_and_relay(&mut child, io)?;
                Ok(status)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let _ = writeln!(io.stderr, "{name}: command not found");
                Ok(127)
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                let _ = writeln!(io.stderr, "{name}: permission denied");
                Ok(126)
            }
            Err(e) => Err(ExecError::Io(e)),
        }
    }

    /// Resolve a path relative to the executor's CWD.
    pub(super) fn resolve_path(&self, path: &str) -> std::path::PathBuf {
        #[cfg(windows)]
        if path == "/dev/null" {
            return std::path::PathBuf::from("NUL");
        }
        let p = std::path::Path::new(path);
        if p.is_relative() {
            self.env.cwd().join(p)
        } else {
            p.to_path_buf()
        }
    }
}
