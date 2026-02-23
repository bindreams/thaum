//! Special builtins: `eval`, `source`/`.`, and `exec`.
//!
//! These builtins need access to the full `Executor` (not just `Environment`),
//! so they are intercepted in `execute_command` before the normal builtin
//! dispatch path.

use crate::exec::error::ExecError;
use crate::exec::io_context::IoContext;
use crate::exec::Executor;

impl Executor {
    /// `eval` builtin: concatenate arguments, parse as shell code, execute.
    ///
    /// Variables, functions, and state changes persist in the current shell
    /// (unlike a subshell).
    pub(super) fn builtin_eval(&mut self, args: &[String], io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        let text = args.join(" ");
        if text.is_empty() {
            return Ok(0);
        }
        let dialect = crate::Dialect::Bash;
        match crate::parse_with(&text, dialect) {
            Ok(program) => self.execute_lines(&program.lines, io),
            Err(_) => Ok(2),
        }
    }

    /// `source` / `.` builtin: read and execute a file in the current shell.
    ///
    /// If extra arguments are supplied after the filename, they temporarily
    /// replace the positional parameters for the duration of the sourced file.
    pub(super) fn builtin_source(&mut self, args: &[String], io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        if args.is_empty() {
            let _ = writeln!(io.stderr, "source: filename argument required");
            return Ok(2);
        }
        let filename = &args[0];

        // Resolve path: if contains '/', use as-is; otherwise search PATH.
        let path = if filename.contains('/') {
            let p = std::path::PathBuf::from(filename);
            if p.is_relative() {
                self.env.cwd().join(p)
            } else {
                p
            }
        } else {
            self.find_in_path(filename)?
        };

        let source = std::fs::read_to_string(&path).map_err(|e| {
            ExecError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{}: {}", filename, e),
            ))
        })?;

        let dialect = crate::Dialect::Bash;
        let program = match crate::parse_with(&source, dialect) {
            Ok(p) => p,
            Err(_) => return Ok(2),
        };

        // Save and set positional params.
        let old_params = self.env.positional_params().to_vec();
        let old_name = self.env.program_name().to_string();
        if args.len() > 1 {
            self.env.set_positional_params(args[1..].to_vec());
        }
        self.env.set_program_name(path.display().to_string());

        let result = self.execute_lines(&program.lines, io);

        // Restore.
        self.env.set_positional_params(old_params);
        self.env.set_program_name(old_name);

        result
    }

    /// `exec` builtin: replace the current shell with the given command.
    ///
    /// With no arguments, redirections take effect on the current shell
    /// (redirect-only mode, not yet fully implemented).
    ///
    /// On Unix, uses `CommandExt::exec()` which replaces the process image.
    /// In tests and on non-Unix, spawns the child and exits via
    /// `ExitRequested`.
    pub(super) fn builtin_exec(&mut self, args: &[String], io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        if args.is_empty() {
            // Redirect-only mode -- redirects were already applied by the
            // caller before we got here. Nothing else to do.
            return Ok(0);
        }

        // Parse flags.
        let mut cmd_start = 0;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--" {
                cmd_start = i + 1;
                break;
            } else if args[i] == "-a" && i + 1 < args.len() {
                // -a name: set argv[0] (Unix only, ignored here for now).
                i += 2;
                cmd_start = i;
                continue;
            } else if args[i].starts_with('-') {
                i += 1;
                cmd_start = i;
                continue;
            } else {
                cmd_start = i;
                break;
            }
        }

        if cmd_start >= args.len() {
            return Ok(0); // No command after flags.
        }

        let cmd_name = &args[cmd_start];
        let cmd_args = &args[cmd_start + 1..];

        // Build the command.
        let mut command = std::process::Command::new(cmd_name);
        command.args(cmd_args);
        command.current_dir(self.env.cwd());

        // Set up environment.
        command.env_clear();
        for (key, val) in self.env.exported_vars() {
            command.env(&key, &val);
        }

        // On Unix: use exec() to replace the process (never returns on success).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = command.exec();
            // exec() only returns on error.
            let _ = writeln!(io.stderr, "exec: {}: {}", cmd_name, err);
            Err(ExecError::ExitRequested(127))
        }

        // On non-Unix: spawn + exit.
        #[cfg(not(unix))]
        {
            match command.status() {
                Ok(status) => Err(ExecError::ExitRequested(status.code().unwrap_or(128))),
                Err(e) => {
                    let _ = writeln!(io.stderr, "exec: {}: {}", cmd_name, e);
                    Err(ExecError::ExitRequested(127))
                }
            }
        }
    }

    /// Search for a command name in `$PATH` directories.
    ///
    /// Returns the full path to the first executable match, or an error if
    /// the command is not found.
    fn find_in_path(&self, name: &str) -> Result<std::path::PathBuf, ExecError> {
        let path_var = self.env.get_var("PATH").unwrap_or("");
        for dir in path_var.split(':') {
            let candidate = std::path::Path::new(dir).join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        Err(ExecError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{}: No such file or directory", name),
        )))
    }
}
