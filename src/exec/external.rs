//! External (non-builtin) command execution via fork/exec. Sets up redirections,
//! exported environment variables, and extra FD mappings before spawning.
//!
//! In **capturing mode** (`IoContext::capturing == true`, used by `CapturedIo`
//! in tests), stdout and stderr are piped and relayed through `IoContext`.
//! In **live mode** (`capturing == false`, used by `ProcessIo`), external
//! commands inherit the parent's stdout/stderr handles directly, which is
//! required for interactive programs and real-time output.

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

        // In capturing mode (tests / CapturedIo), pipe stdout/stderr so we
        // can relay output through IoContext. In live mode (ProcessIo),
        // let the child inherit parent handles directly — required for
        // interactive programs like cmd.exe / pwsh.exe and for real-time
        // output from long-running commands.
        if io.capturing {
            child_cmd.fds.entry(1).or_insert(Fd::Pipe);
            child_cmd.fds.entry(2).or_insert(Fd::Pipe);
        }

        match child_cmd.spawn() {
            Ok(mut child) => {
                let (stdout_buf, stderr_buf) = child_io::drain_child_pipes(&mut child)?;
                let status = child.wait().map_err(ExecError::Io)?;
                if !stdout_buf.is_empty() {
                    io.stdout.write_all(&stdout_buf).map_err(ExecError::Io)?;
                }
                if !stderr_buf.is_empty() {
                    io.stderr.write_all(&stderr_buf).map_err(ExecError::Io)?;
                }
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
