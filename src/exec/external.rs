use std::process::Stdio;

use crate::ast::{Redirect, RedirectKind};
use crate::exec::error::ExecError;
use crate::exec::expand;
use crate::exec::io_context::IoContext;
use crate::exec::Executor;

impl Executor {
    /// Execute an external command via fork/exec.
    pub(super) fn execute_external(
        &mut self,
        name: &str,
        args: &[String],
        assignments: &[crate::ast::Assignment],
        redirects: &[Redirect],
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        let mut child_cmd = std::process::Command::new(name);
        child_cmd.args(args);
        child_cmd.current_dir(self.env.cwd());

        // Set environment from exported variables
        child_cmd.env_clear();
        for (key, val) in self.env.exported_vars() {
            child_cmd.env(&key, &val);
        }

        // Apply prefix assignments as extra env vars
        for assignment in assignments {
            let value = self.expand_word(&assignment.value.as_scalar())?;
            child_cmd.env(&assignment.name, &value);
        }

        // Apply redirections
        self.apply_redirects_to_command(&mut child_cmd, redirects)?;

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

    /// Apply redirections to a std::process::Command.
    pub(super) fn apply_redirects_to_command(
        &self,
        child_cmd: &mut std::process::Command,
        redirects: &[Redirect],
    ) -> Result<(), ExecError> {
        for redirect in redirects {
            let fd = redirect.fd;
            match &redirect.kind {
                RedirectKind::Input(word) => {
                    let path = expand::expand_word(word, &self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = std::fs::File::open(&resolved).map_err(|e| {
                        ExecError::BadRedirect(format!("{}: {}", path, e))
                    })?;
                    match fd.unwrap_or(0) {
                        0 => { child_cmd.stdin(file); }
                        fd => {
                            return Err(ExecError::UnsupportedFeature(format!(
                                "input redirect on fd {}",
                                fd,
                            )));
                        }
                    }
                }
                RedirectKind::Output(word) | RedirectKind::Clobber(word) => {
                    let path = expand::expand_word(word, &self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = std::fs::File::create(&resolved).map_err(|e| {
                        ExecError::BadRedirect(format!("{}: {}", path, e))
                    })?;
                    match fd.unwrap_or(1) {
                        1 => { child_cmd.stdout(file); }
                        2 => { child_cmd.stderr(file); }
                        fd => {
                            return Err(ExecError::UnsupportedFeature(format!(
                                "output redirect on fd {}",
                                fd,
                            )));
                        }
                    }
                }
                RedirectKind::Append(word) => {
                    let path = expand::expand_word(word, &self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&resolved)
                        .map_err(|e| ExecError::BadRedirect(format!("{}: {}", path, e)))?;
                    match fd.unwrap_or(1) {
                        1 => { child_cmd.stdout(file); }
                        2 => { child_cmd.stderr(file); }
                        fd => {
                            return Err(ExecError::UnsupportedFeature(format!(
                                "append redirect on fd {}",
                                fd,
                            )));
                        }
                    }
                }
                RedirectKind::HereDoc { .. } => {
                    return Err(ExecError::UnsupportedFeature(
                        "heredoc redirect".to_string(),
                    ));
                }
                RedirectKind::DupInput(word) => {
                    let target = expand::expand_word(word, &self.env)?;
                    if target == "-" {
                        child_cmd.stdin(Stdio::null());
                    } else {
                        return Err(ExecError::UnsupportedFeature(
                            "fd duplication".to_string(),
                        ));
                    }
                }
                RedirectKind::DupOutput(word) => {
                    let target = expand::expand_word(word, &self.env)?;
                    if target == "-" {
                        match fd.unwrap_or(1) {
                            1 => { child_cmd.stdout(Stdio::null()); }
                            2 => { child_cmd.stderr(Stdio::null()); }
                            _ => {}
                        }
                    } else {
                        return Err(ExecError::UnsupportedFeature(
                            "fd duplication".to_string(),
                        ));
                    }
                }
                _ => {
                    return Err(ExecError::UnsupportedFeature(
                        "redirect kind not implemented".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Resolve a path relative to the executor's CWD.
    pub(super) fn resolve_path(&self, path: &str) -> std::path::PathBuf {
        let p = std::path::Path::new(path);
        if p.is_relative() {
            self.env.cwd().join(p)
        } else {
            p.to_path_buf()
        }
    }
}
