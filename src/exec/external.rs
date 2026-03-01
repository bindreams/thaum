//! External (non-builtin) command execution via fork/exec. Sets up redirections,
//! exported environment variables, and extra FD mappings before spawning.

use std::process::Stdio;

use crate::exec::command_ex::CommandEx;
use crate::exec::error::ExecError;
use crate::exec::io_context::IoContext;
use crate::exec::redirect::ActiveRedirects;
use crate::exec::Executor;

impl Executor {
    /// Execute an external command via fork/exec.
    ///
    /// Redirections are pre-resolved in `active`. FDs 0-2 are set on the child
    /// via `Stdio::from(file)`. FDs 3+ are passed via `CommandEx::fd_mapping()`.
    /// The original `io` is used only for error messages (command not found,
    /// permission denied).
    pub(super) fn execute_external(
        &mut self,
        name: &str,
        args: &[String],
        assignments: &[crate::ast::Assignment],
        active: &mut ActiveRedirects,
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        let mut child_cmd = CommandEx::new(name);
        child_cmd.args(args);
        child_cmd.current_dir(self.env.cwd());

        // Set environment from exported variables
        child_cmd.env_clear();
        for (key, val) in self.env.exported_vars() {
            child_cmd.env(&key, &val);
        }

        // Apply prefix assignments as extra env vars
        for assignment in assignments {
            let value = self.expand_scalar_assignment(assignment)?;
            child_cmd.env(&assignment.name, &value);
        }

        // Apply FDs 0-2 from pre-resolved redirects
        if let Some(ref file) = active.stdin {
            child_cmd.stdin(Stdio::from(file.try_clone().map_err(ExecError::Io)?));
        }
        if let Some(ref file) = active.stdout {
            child_cmd.stdout(Stdio::from(file.try_clone().map_err(ExecError::Io)?));
        }
        if let Some(ref file) = active.stderr {
            child_cmd.stderr(Stdio::from(file.try_clone().map_err(ExecError::Io)?));
        }

        // FDs 3+ from command-level redirects and persistent fd_table
        for (&fd, file) in self.fd_table.iter().chain(active.extra_fds.iter()) {
            child_cmd.fd_mapping(fd, file.try_clone().map_err(ExecError::Io)?);
        }

        match child_cmd.spawn() {
            Ok(mut child) => {
                let status = child.wait().map_err(ExecError::Io)?;
                Ok(status.code().unwrap_or(128))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let _ = writeln!(io.stderr, "{}: command not found", name);
                Ok(127)
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                let _ = writeln!(io.stderr, "{}: permission denied", name);
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
