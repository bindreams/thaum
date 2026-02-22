pub mod arithmetic;
pub mod builtins;
pub(crate) mod command_ex;
mod compound;
pub mod environment;
pub mod error;
pub mod expand;
mod external;
pub mod io_context;
mod pattern;
pub mod pipeline;
mod redirect;

pub use environment::Environment;
pub use error::ExecError;
pub use io_context::{CapturedIo, IoContext, ProcessIo};
#[cfg(test)]
use pattern::shell_pattern_match;

use std::collections::HashMap;
use std::process::Stdio;

use crate::ast::{Command, ExecutionMode, Expression, Program, Statement};

/// The shell executor.
///
/// Takes a parsed AST and executes it, maintaining shell state (variables,
/// functions, CWD, exit status) in an `Environment`.
pub struct Executor {
    env: Environment,
    /// Persistent extra file descriptors (3+), typically set by `exec N>file`.
    /// FDs 0-2 are handled by IoContext; this table holds FDs 3 and above.
    fd_table: HashMap<i32, std::fs::File>,
}

impl Executor {
    /// Create a new executor with default environment.
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.inherit_from_process();
        Executor {
            env,
            fd_table: HashMap::new(),
        }
    }

    /// Create an executor with a specific environment.
    pub fn with_env(env: Environment) -> Self {
        Executor {
            env,
            fd_table: HashMap::new(),
        }
    }

    /// Get a mutable reference to the environment.
    pub fn env_mut(&mut self) -> &mut Environment {
        &mut self.env
    }

    /// Get a reference to the environment.
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Get a reference to the persistent FD table.
    pub fn fd_table(&self) -> &HashMap<i32, std::fs::File> {
        &self.fd_table
    }

    /// Execute a parsed program. Returns the exit status of the last command.
    pub fn execute(&mut self, program: &Program, io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        self.execute_lines(&program.lines, io)
    }

    /// Execute a list of lines, returning the last exit status.
    pub fn execute_lines(
        &mut self,
        lines: &[crate::ast::Line],
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        let mut status = 0;
        for line in lines {
            status = self.execute_statements(line, io)?;
        }
        Ok(status)
    }

    /// Execute a list of statements, returning the last exit status.
    pub fn execute_statements(
        &mut self,
        stmts: &[Statement],
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        let mut status = 0;
        for stmt in stmts {
            status = self.execute_statement(stmt, io)?;
        }
        Ok(status)
    }

    /// Execute a single statement.
    fn execute_statement(
        &mut self,
        stmt: &Statement,
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        match stmt.mode {
            ExecutionMode::Background => Err(ExecError::UnsupportedFeature(
                "background execution (&)".to_string(),
            )),
            ExecutionMode::Sequential | ExecutionMode::Terminated => {
                let status = self.execute_expression(&stmt.expression, io)?;
                self.env.set_last_exit_status(status);
                Ok(status)
            }
        }
    }

    /// Execute an expression, returning its exit status.
    pub fn execute_expression(
        &mut self,
        expr: &Expression,
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        match expr {
            Expression::Command(cmd) => self.execute_command(cmd, io),

            Expression::Compound { body, redirects } => self.execute_compound(body, redirects, io),

            Expression::FunctionDef(fndef) => {
                let stored = environment::StoredFunction::from(fndef);
                self.env.set_function(fndef.name.clone(), stored);
                Ok(0)
            }

            Expression::And { left, right } => {
                let left_status = self.execute_expression(left, io)?;
                self.env.set_last_exit_status(left_status);
                if left_status == 0 {
                    self.execute_expression(right, io)
                } else {
                    Ok(left_status)
                }
            }

            Expression::Or { left, right } => {
                let left_status = self.execute_expression(left, io)?;
                self.env.set_last_exit_status(left_status);
                if left_status != 0 {
                    self.execute_expression(right, io)
                } else {
                    Ok(0)
                }
            }

            Expression::Pipe { .. } => {
                let stages = pipeline::flatten_pipeline(expr);
                pipeline::execute_pipeline(self, &stages, io)
            }

            Expression::Not(inner) => {
                let status = self.execute_expression(inner, io)?;
                Ok(if status == 0 { 1 } else { 0 })
            }
        }
    }

    /// Expand a word, resolving command substitutions first.
    fn expand_word(&mut self, word: &crate::ast::Word) -> Result<String, ExecError> {
        let resolved = self.resolve_cmd_subs_in_word(word)?;
        expand::expand_word(&resolved, &mut self.env)
    }

    /// Expand an argument, resolving command substitutions first.
    fn expand_argument(&mut self, arg: &crate::ast::Argument) -> Result<Vec<String>, ExecError> {
        match arg {
            crate::ast::Argument::Word(word) => {
                let resolved = self.resolve_cmd_subs_in_word(word)?;
                expand::expand_word_to_fields(&resolved, &mut self.env)
            }
            crate::ast::Argument::Atom(atom) => match atom {
                crate::ast::Atom::BashProcessSubstitution { .. } => {
                    Err(ExecError::BadSubstitution(
                        "process substitution not supported in POSIX mode".to_string(),
                    ))
                }
            },
        }
    }

    /// Expand a word into fields, resolving command substitutions first.
    fn expand_word_to_fields(&mut self, word: &crate::ast::Word) -> Result<Vec<String>, ExecError> {
        let resolved = self.resolve_cmd_subs_in_word(word)?;
        expand::expand_word_to_fields(&resolved, &mut self.env)
    }

    /// Pre-process a Word, executing command substitutions and replacing
    /// them with Literal fragments containing the captured output.
    fn resolve_cmd_subs_in_word(
        &mut self,
        word: &crate::ast::Word,
    ) -> Result<crate::ast::Word, ExecError> {
        let new_parts = self.resolve_cmd_subs_in_fragments(&word.parts)?;
        Ok(crate::ast::Word {
            parts: new_parts,
            span: word.span,
        })
    }

    /// Resolve command substitutions in a list of fragments.
    fn resolve_cmd_subs_in_fragments(
        &mut self,
        fragments: &[crate::ast::Fragment],
    ) -> Result<Vec<crate::ast::Fragment>, ExecError> {
        use crate::ast::Fragment;
        let mut result = Vec::with_capacity(fragments.len());
        for fragment in fragments {
            match fragment {
                Fragment::CommandSubstitution(stmts) => {
                    let output = self.execute_command_substitution(stmts)?;
                    // Strip trailing newlines (POSIX behavior)
                    let trimmed = output.trim_end_matches('\n').to_string();
                    result.push(Fragment::Literal(trimmed));
                }
                Fragment::ArithmeticExpansion(expr) => {
                    let value = arithmetic::evaluate_arith_expr(expr, &mut self.env)?;
                    result.push(Fragment::Literal(value.to_string()));
                }
                Fragment::DoubleQuoted(parts) => {
                    let resolved = self.resolve_cmd_subs_in_fragments(parts)?;
                    result.push(Fragment::DoubleQuoted(resolved));
                }
                // All other fragments pass through unchanged
                other => result.push(other.clone()),
            }
        }
        Ok(result)
    }

    /// Execute a command substitution ($(...)), capturing stdout.
    ///
    /// Runs the statements and captures stdout. For builtins, captures
    /// the output buffer. For external commands, uses piped stdout.
    /// Creates its own internal IoContext with a capture buffer for stdout
    /// and io::sink() for stderr (discarding stderr from command substitutions).
    fn execute_command_substitution(&mut self, stmts: &[Statement]) -> Result<String, ExecError> {
        let mut captured = Vec::new();

        for stmt in stmts {
            match &stmt.expression {
                Expression::Command(cmd) => {
                    let mut args: Vec<String> = Vec::new();
                    for arg in &cmd.arguments {
                        let fields = expand::expand_argument(arg, &mut self.env)?;
                        args.extend(fields);
                    }

                    if args.is_empty() {
                        for assignment in &cmd.assignments {
                            self.execute_assignment(assignment)?;
                        }
                        continue;
                    }

                    let cmd_name = &args[0];
                    let cmd_args = &args[1..];

                    if builtins::is_builtin(cmd_name) {
                        let mut stderr_buf = Vec::new();
                        let mut sink_stdin = std::io::empty();
                        let status = builtins::run_builtin(
                            cmd_name,
                            cmd_args,
                            &mut self.env,
                            &mut sink_stdin,
                            &mut captured,
                            &mut stderr_buf,
                        );
                        match status {
                            Ok(s) => self.env.set_last_exit_status(s),
                            Err(ExecError::ExitRequested(code)) => {
                                self.env.set_last_exit_status(code);
                                break;
                            }
                            Err(e) => return Err(e),
                        }
                    } else {
                        let mut child_cmd = std::process::Command::new(cmd_name);
                        child_cmd.args(cmd_args);
                        child_cmd.current_dir(self.env.cwd());
                        child_cmd.stdout(Stdio::piped());
                        child_cmd.env_clear();
                        for (key, val) in self.env.exported_vars() {
                            child_cmd.env(&key, &val);
                        }

                        match child_cmd.output() {
                            Ok(output) => {
                                captured.extend_from_slice(&output.stdout);
                                let code = output.status.code().unwrap_or(128);
                                self.env.set_last_exit_status(code);
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                self.env.set_last_exit_status(127);
                            }
                            Err(e) => return Err(ExecError::Io(e)),
                        }
                    }
                }
                _ => {
                    return Err(ExecError::UnsupportedFeature(
                        "compound command in command substitution".to_string(),
                    ));
                }
            }
        }

        Ok(String::from_utf8_lossy(&captured).to_string())
    }

    /// Execute a simple command.
    fn execute_command(&mut self, cmd: &Command, io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        // Expand arguments
        let mut expanded_args: Vec<String> = Vec::new();
        for arg in &cmd.arguments {
            let fields = self.expand_argument(arg)?;
            expanded_args.extend(fields);
        }

        // Resolve redirects before dispatch — applies to all command types.
        // Files are opened here (side effect: `> file` creates/truncates even
        // without a command). Redirect handles are dropped when `active` goes
        // out of scope.
        let mut active = self.resolve_redirects(&cmd.redirects)?;

        // If no command name, just process assignments
        if expanded_args.is_empty() {
            for assignment in &cmd.assignments {
                self.execute_assignment(assignment)?;
            }
            return Ok(0);
        }

        let cmd_name = &expanded_args[0];
        let cmd_args = &expanded_args[1..];

        // Check for functions first
        if let Some(func) = self.env.get_function(cmd_name).cloned() {
            let saved = self.apply_prefix_assignments(&cmd.assignments)?;
            self.env.push_scope(cmd_args.to_vec());

            let mut cmd_io = active.apply_to_io(io);
            let result = self.execute_compound(&func.body, &func.redirects, &mut cmd_io);

            self.env.pop_scope();
            self.restore_prefix_assignments(saved);

            return match result {
                Ok(status) => Ok(status),
                Err(ExecError::ReturnRequested(code)) => Ok(code),
                Err(e) => Err(e),
            };
        }

        // Check for builtins
        if builtins::is_builtin(cmd_name) {
            let saved = self.apply_prefix_assignments(&cmd.assignments)?;

            let cmd_io = active.apply_to_io(io);
            let mut stdout_buf: Vec<u8> = Vec::new();
            let mut stderr_buf: Vec<u8> = Vec::new();

            let result = builtins::run_builtin(
                cmd_name,
                cmd_args,
                &mut self.env,
                cmd_io.stdin,
                &mut stdout_buf,
                &mut stderr_buf,
            );

            // Write captured output to the (possibly redirected) io context
            if !stdout_buf.is_empty() {
                cmd_io
                    .stdout
                    .write_all(&stdout_buf)
                    .map_err(ExecError::Io)?;
            }
            if !stderr_buf.is_empty() {
                cmd_io
                    .stderr
                    .write_all(&stderr_buf)
                    .map_err(ExecError::Io)?;
            }

            self.restore_prefix_assignments(saved);

            return result;
        }

        // External command — pass ActiveRedirects for Stdio setup;
        // `io` is used only for error messages (not redirected).
        self.execute_external(cmd_name, cmd_args, &cmd.assignments, &mut active, io)
    }

    /// Execute a full assignment (scalar, indexed, or array).
    pub(crate) fn execute_assignment(
        &mut self,
        assignment: &crate::ast::Assignment,
    ) -> Result<(), ExecError> {
        if let Some(ref subscript) = assignment.index {
            // Indexed assignment: name[subscript]=value
            let value = self.expand_word(assignment.value.as_scalar())?;
            let index: usize = subscript.parse().unwrap_or(0);
            self.env
                .set_array_element(&assignment.name, index, &value)?;
        } else {
            match &assignment.value {
                crate::ast::AssignmentValue::Scalar(word) => {
                    let value = self.expand_word(word)?;
                    self.env.set_var(&assignment.name, &value)?;
                }
                crate::ast::AssignmentValue::BashArray(words) => {
                    let mut elements = Vec::new();
                    for word in words {
                        elements.push(self.expand_word(word)?);
                    }
                    self.env.set_array(&assignment.name, elements)?;
                }
            }
        }
        Ok(())
    }

    /// Expand a scalar assignment value. Panics on BashArray (use for prefix
    /// assignments where arrays are not valid).
    pub(crate) fn expand_scalar_assignment(
        &mut self,
        assignment: &crate::ast::Assignment,
    ) -> Result<String, ExecError> {
        self.expand_word(assignment.value.as_scalar())
    }

    /// Apply prefix assignments temporarily, returning saved values.
    fn apply_prefix_assignments(
        &mut self,
        assignments: &[crate::ast::Assignment],
    ) -> Result<Vec<(String, Option<String>)>, ExecError> {
        let mut saved = Vec::new();
        for assignment in assignments {
            let old = self.env.get_var(&assignment.name).map(|s| s.to_string());
            let value = self.expand_scalar_assignment(assignment)?;
            self.env.set_var(&assignment.name, &value)?;
            saved.push((assignment.name.clone(), old));
        }
        Ok(saved)
    }

    /// Restore prefix assignments from saved values.
    fn restore_prefix_assignments(&mut self, saved: Vec<(String, Option<String>)>) {
        for (name, old_val) in saved {
            match old_val {
                Some(val) => {
                    let _ = self.env.set_var(&name, &val);
                }
                None => {
                    let _ = self.env.unset_var(&name);
                }
            }
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: parse and execute a script string.
pub fn run(input: &str) -> Result<i32, ExecError> {
    let program = crate::parse(input)
        .map_err(|e| ExecError::BadSubstitution(format!("parse error: {}", e)))?;
    let mut executor = Executor::new();
    let mut process_io = ProcessIo::new();
    match executor.execute(&program, &mut process_io.context()) {
        Ok(status) => Ok(status),
        Err(ExecError::ExitRequested(code)) => Ok(code),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[path = "exec/tests.rs"]
mod tests;
