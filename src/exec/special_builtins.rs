//! Special builtins: `eval`, `source`/`.`, and `exec`.
//!
//! These builtins need access to the full `Executor` (not just `Environment`),
//! so they are intercepted in `execute_command` before the normal builtin
//! dispatch path.

use crate::exec::command_ex::{CommandEx, Fd};
use crate::exec::error::ExecError;
use crate::exec::io_context::IoContext;
use crate::exec::redirect::ActiveRedirects;
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
        match crate::parse_with_options(&text, self.options.clone()) {
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

        let program = match crate::parse_with_options(&source, self.options.clone()) {
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
    /// With no arguments, redirect-only mode is handled by the caller
    /// (`execute_command`) which adopts the redirects into the persistent
    /// `fd_table` before reaching this function.
    ///
    /// On Unix, replaces the process image via `execvp`. On other platforms,
    /// spawns the child and exits via `ExitRequested`.
    pub(super) fn builtin_exec(
        &mut self,
        args: &[String],
        active: &mut ActiveRedirects,
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        if args.is_empty() {
            // Redirect-only mode is handled before this function is called.
            return Ok(0);
        }

        // Parse flags.
        let mut argv0_override: Option<&str> = None;
        let mut cmd_start = 0;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--" {
                cmd_start = i + 1;
                break;
            } else if args[i] == "-a" && i + 1 < args.len() {
                argv0_override = Some(&args[i + 1]);
                i += 2;
                cmd_start = i;
                continue;
            } else if args[i].starts_with('-') {
                let _ = writeln!(io.stderr, "exec: {}: invalid option", args[i]);
                return Ok(2);
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

        // Build argv: [argv0, arg1, arg2, ...].
        let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(1 + cmd_args.len());
        argv.push(argv0_override.unwrap_or(cmd_name).into());
        argv.extend(cmd_args.iter().map(std::ffi::OsString::from));

        let mut cmd = CommandEx::new(argv);
        cmd.path = cmd_name.into();
        cmd.cwd = Some(self.env.cwd().to_path_buf());
        cmd.env = self
            .env
            .exported_vars()
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        // Build FD table: persistent fd_table first, per-command redirects override.
        for (&fd, file) in &self.fd_table {
            cmd.fds.insert(fd, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }
        if let Some(ref file) = active.stdin {
            cmd.fds.insert(0, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }
        if let Some(ref file) = active.stdout {
            cmd.fds.insert(1, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }
        if let Some(ref file) = active.stderr {
            cmd.fds.insert(2, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }
        for (&fd, file) in &active.extra_fds {
            cmd.fds.insert(fd, Fd::File(file.try_clone().map_err(ExecError::Io)?));
        }

        #[cfg(unix)]
        {
            // Replace the process image. Only returns on error.
            let e = cmd.exec_replace();
            let _ = writeln!(io.stderr, "exec: {}: {}", cmd_name, e);
            Err(ExecError::ExitRequested(127))
        }

        #[cfg(not(unix))]
        {
            match cmd.spawn() {
                Ok(mut child) => {
                    let code = child.wait().map_err(ExecError::Io)?;
                    Err(ExecError::ExitRequested(code))
                }
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
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(sep) {
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
