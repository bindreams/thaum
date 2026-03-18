//! External (non-builtin) command execution via fork/exec. Sets up redirections,
//! exported environment variables, and extra FD mappings before spawning.
//!
//! For unredirected stdout/stderr, the spawning strategy depends on the IoContext
//! and `Executor::terminal_inherit`:
//! - `terminal_inherit` + tty fd: the child inherits the parent's terminal directly
//!   (no interposing pipe or PTY), so interactive programs work correctly.
//! - tty fd without `terminal_inherit`: a PTY is used so `isatty()` returns true
//!   in the child while output is still captured and relayed through IoContext.
//! - non-tty fd: a plain pipe is used; output is relayed through IoContext.

use std::io::Write;

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
    /// through `io` so that `CapturedIo` (tests) and `IoContext::from_process()` (live) both
    /// receive the child's output.
    pub(super) fn execute_external(
        &mut self,
        name: &str,
        args: &[String],
        assignments: &[crate::ast::Assignment],
        active: &mut ActiveRedirects,
        io: &mut IoContext,
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

        // IoContext FDs (3+) — includes persistent fds from `exec` redirects.
        for (&fd, file) in io.fds() {
            if fd >= 3 {
                child_cmd
                    .fds
                    .insert(fd, Fd::File(file.try_clone().map_err(ExecError::Io)?));
            }
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

        // Set up stdout/stderr for the child. For unredirected fds:
        // - terminal_inherit + tty: skip insertion → child inherits parent's
        //   terminal directly (avoids ConPTY stdin freeze for interactive cmds).
        // - no terminal_inherit + tty: use Fd::Pty so child sees isatty()==true.
        // - no tty: use Fd::Pipe for capture.
        // `entry().or_insert` respects explicit redirects above.
        for &fd in &[1, 2] {
            if let std::collections::hash_map::Entry::Vacant(e) = child_cmd.fds.entry(fd) {
                if self.terminal_inherit && io.is_tty(fd) {
                    // Child inherits parent's terminal fd directly.
                } else {
                    e.insert(if io.is_tty(fd) { Fd::Pty } else { Fd::Pipe });
                }
            }
        }

        match child_cmd.spawn() {
            Ok(mut child) => {
                let status = child_io::drain_and_relay(&mut child, io)?;
                Ok(status)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(stderr) = io.fd_mut(2) {
                    let _ = writeln!(stderr, "{name}: command not found");
                }
                Ok(127)
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                if let Some(stderr) = io.fd_mut(2) {
                    let _ = writeln!(stderr, "{name}: permission denied");
                }
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
