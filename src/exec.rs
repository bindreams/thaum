//! Shell executor: walks the AST and runs commands.
//!
//! The `Executor` owns an `Environment` (variables, functions, aliases, CWD)
//! and dispatches to builtins, external processes, and compound-command
//! handlers. Alias expansion happens here at execution time -- the parser
//! never sees aliases.

/// Runtime evaluation of `ArithExpr` nodes (`(( ))`, `$(( ))`, `for (( ; ; ))`).
pub mod arithmetic;
pub(crate) mod bash_test;
/// Shell builtins that only need `Environment` (not the full `Executor`).
pub mod builtins;
pub(crate) mod command_ex;
mod compound;
/// Shell state: variables, functions, aliases, positional parameters, CWD, `$?`.
pub mod environment;
/// Execution errors and control-flow signals (`exit`, `break`, `return`).
pub mod error;
/// POSIX word expansion: tilde, parameter, quote removal.
pub mod expand;
mod external;
/// GNU gettext catalog lookup for `$"..."` locale translation.
pub(crate) mod gettext;
/// Pluggable I/O context for stdin/stdout/stderr (live or captured).
pub mod io_context;
/// Locale-aware string operations (case conversion) via ICU4X.
pub(crate) mod locale;
pub(crate) mod numeric;
mod pattern;
/// Pipeline execution: flatten pipe trees, spawn stages with piped I/O.
pub mod pipeline;
mod platform;
pub(crate) mod printf;
pub mod redirect;
mod special_builtins;
/// Subshell serialization payload for cross-process `thaum exec-ast`.
pub mod subshell;

pub use environment::Environment;
pub use error::ExecError;
pub use io_context::{CapturedIo, IoContext, ProcessIo};
#[cfg(test)]
use pattern::shell_pattern_match;

use std::collections::{HashMap, HashSet};
use std::io::Write;

use crate::ast::{Command, ExecutionMode, Expression, Line, Program, Statement};

/// The shell executor.
///
/// Takes a parsed AST and executes it, maintaining shell state (variables,
/// functions, CWD, exit status) in an `Environment`.
pub struct Executor {
    env: Environment,
    /// Persistent extra file descriptors (3+), typically set by `exec N>file`.
    /// FDs 0-2 are handled by IoContext; this table holds FDs 3 and above.
    fd_table: HashMap<i32, std::fs::File>,
    /// Alias table snapshot taken at the start of each line.  Alias expansion
    /// uses this snapshot so that `alias`/`unalias` within a line don't affect
    /// expansion of later commands on the same line (matching bash semantics).
    alias_snapshot: HashMap<String, String>,
    /// Path to the thaum binary for subshell spawning.  Defaults to
    /// `std::env::current_exe()`.  Override for testing.
    exe_path: Option<std::path::PathBuf>,
    /// Suppresses errexit (`set -e`) in guarded contexts: `if`/`while`/`until`
    /// conditions, and the left-hand side of `&&`/`||` and `!` operands.
    errexit_suppressed: bool,
    /// Shell options (dialect features) controlling which builtins and syntax
    /// extensions are available at execution time.
    options: crate::dialect::ShellOptions,
}

impl Executor {
    /// Create a new executor with default environment (Bash mode for backwards
    /// compatibility).
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.inherit_from_process();
        Executor {
            env,
            fd_table: HashMap::new(),
            alias_snapshot: HashMap::new(),
            exe_path: None,
            errexit_suppressed: false,
            options: crate::Dialect::Bash.options(),
        }
    }

    /// Create an executor with specific shell options (dialect features).
    pub fn with_options(options: crate::dialect::ShellOptions) -> Self {
        let mut env = Environment::new();
        env.inherit_from_process();
        env.set_array_empty_element_alternative_bug(options.array_empty_element_alternative_bug);
        env.set_typeset_can_unset_readonly(options.typeset_can_unset_readonly);
        if options.declare_builtin {
            env.initialize_bash_vars();
        }
        Executor {
            env,
            fd_table: HashMap::new(),
            alias_snapshot: HashMap::new(),
            exe_path: None,
            errexit_suppressed: false,
            options,
        }
    }

    /// Create an executor with a specific environment (Bash mode).
    pub fn with_env(env: Environment) -> Self {
        Executor {
            env,
            fd_table: HashMap::new(),
            alias_snapshot: HashMap::new(),
            exe_path: None,
            errexit_suppressed: false,
            options: crate::Dialect::Bash.options(),
        }
    }

    /// Create an executor with a specific environment and shell options.
    pub fn with_env_and_options(mut env: Environment, options: crate::dialect::ShellOptions) -> Self {
        env.set_array_empty_element_alternative_bug(options.array_empty_element_alternative_bug);
        env.set_typeset_can_unset_readonly(options.typeset_can_unset_readonly);
        if options.declare_builtin {
            env.initialize_bash_vars();
        }
        Executor {
            env,
            fd_table: HashMap::new(),
            alias_snapshot: HashMap::new(),
            exe_path: None,
            errexit_suppressed: false,
            options,
        }
    }

    /// Set the path to the thaum binary for subshell spawning.
    pub fn set_exe_path(&mut self, path: std::path::PathBuf) {
        self.exe_path = Some(path);
    }

    /// Get a mutable reference to the environment.
    pub fn env_mut(&mut self) -> &mut Environment {
        &mut self.env
    }

    /// Get a reference to the environment.
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// The shell options (dialect features) this executor was configured with.
    pub fn options(&self) -> &crate::dialect::ShellOptions {
        &self.options
    }

    /// Get a reference to the persistent FD table.
    pub fn fd_table(&self) -> &HashMap<i32, std::fs::File> {
        &self.fd_table
    }

    /// Mutable access to the persistent FD table. Used by `exec-ast` child
    /// processes to reconstruct inherited FDs.
    pub fn fd_table_mut(&mut self) -> &mut HashMap<i32, std::fs::File> {
        &mut self.fd_table
    }

    /// Adopt redirects from `exec` redirect-only mode into persistent state.
    ///
    /// FDs 0-2 and 3+ are all stored in `fd_table`. Persistent FDs 0-2 are
    /// injected into `ActiveRedirects` at the start of `execute_command`;
    /// FDs 3+ are inherited by child processes via `execute_external` and
    /// pipeline stages. Explicitly closed FDs are removed.
    fn adopt_redirects(&mut self, active: redirect::ActiveRedirects) {
        for fd in &active.closed_fds {
            self.fd_table.remove(fd);
        }
        if let Some(f) = active.stdin {
            self.fd_table.insert(0, f);
        }
        if let Some(f) = active.stdout {
            self.fd_table.insert(1, f);
        }
        if let Some(f) = active.stderr {
            self.fd_table.insert(2, f);
        }
        for (fd, file) in active.extra_fds {
            self.fd_table.insert(fd, file);
        }
    }

    /// Execute a parsed program. Returns the exit status of the last command.
    pub fn execute(&mut self, program: &Program, io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        self.execute_lines(&program.lines, io)
    }

    /// Execute a list of lines, returning the last exit status.
    ///
    /// Takes an alias table snapshot before each line so that alias
    /// definitions within a line only take effect for subsequent lines.
    pub fn execute_lines(&mut self, lines: &[crate::ast::Line], io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        let mut status = 0;
        for (i, line) in lines.iter().enumerate() {
            self.env.set_lineno(i + 1);
            self.alias_snapshot = self.env.alias_snapshot();
            status = self.execute_statements(line, io)?;
        }
        Ok(status)
    }

    /// Execute a subshell by spawning `thaum exec-ast` as a child process.
    ///
    /// Serializes the current environment and the subshell body as JSON,
    /// pipes it to the child's stdin, and captures stdout/stderr.  The child
    /// is a real separate process, so `$BASHPID` and signal isolation work
    /// correctly on all platforms.
    pub(crate) fn execute_subshell(&mut self, body: &[Line], io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        use command_ex::{CommandEx, Fd};
        use std::io::Read;

        let inherited_fds: Vec<i32> = self.fd_table.keys().filter(|&&fd| fd >= 3).copied().collect();

        let payload = subshell::SubshellPayload {
            env: self.env.serialize(),
            body: body.to_vec(),
            options: self.options.clone(),
            inherited_fds: inherited_fds.clone(),
        };
        let json = serde_json::to_string(&payload).map_err(|e| ExecError::Io(std::io::Error::other(e)))?;

        let exe = match &self.exe_path {
            Some(p) => p.clone(),
            None => std::env::current_exe().map_err(ExecError::Io)?,
        };

        let mut cmd = CommandEx::new(vec![exe.into(), "exec-ast".into()]);
        cmd.fds.insert(0, Fd::InputPipe); // parent writes JSON payload
        cmd.fds.insert(1, Fd::Pipe); // parent reads stdout
        cmd.fds.insert(2, Fd::Pipe); // parent reads stderr
        cmd.cwd = Some(self.env.cwd().to_path_buf());
        // Inherit the full process environment so the child has system vars.
        cmd.env = std::env::vars_os().collect();

        // Pass fd_table FDs (3+) to the child atomically.
        for &fd in &inherited_fds {
            if let Some(file) = self.fd_table.get(&fd) {
                cmd.fds.insert(fd, Fd::File(file.try_clone().map_err(ExecError::Io)?));
            }
        }

        let mut child = cmd.spawn().map_err(ExecError::Io)?;

        // Write JSON payload and close the write end so child sees EOF.
        if let Some(mut stdin_pipe) = child.take_pipe(0) {
            stdin_pipe.write_all(json.as_bytes()).map_err(ExecError::Io)?;
        }

        // Read stdout and stderr from the child.
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        if let Some(mut stdout_pipe) = child.take_pipe(1) {
            stdout_pipe.read_to_end(&mut stdout_buf).map_err(ExecError::Io)?;
        }
        if let Some(mut stderr_pipe) = child.take_pipe(2) {
            stderr_pipe.read_to_end(&mut stderr_buf).map_err(ExecError::Io)?;
        }

        let status = child.wait().map_err(ExecError::Io)?;

        io.stdout.write_all(&stdout_buf).map_err(ExecError::Io)?;
        io.stderr.write_all(&stderr_buf).map_err(ExecError::Io)?;

        Ok(status)
    }

    /// Execute a list of statements, returning the last exit status.
    pub fn execute_statements(&mut self, stmts: &[Statement], io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        let mut status = 0;
        for stmt in stmts {
            status = self.execute_statement(stmt, io)?;
        }
        Ok(status)
    }

    /// Execute a single statement.
    fn execute_statement(&mut self, stmt: &Statement, io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        match stmt.mode {
            ExecutionMode::Background => Err(ExecError::UnsupportedFeature("background execution (&)".to_string())),
            ExecutionMode::Sequential | ExecutionMode::Terminated => {
                let saved = self.errexit_suppressed;
                let status = self.execute_expression(&stmt.expression, io)?;
                self.env.set_last_exit_status(status);
                // Update PIPESTATUS for single commands (pipelines set it themselves).
                let _ = self.env.set_array("PIPESTATUS", vec![status.to_string()]);
                // Errexit check: if command failed and -e is active and not suppressed.
                // execute_expression may have set errexit_suppressed=true during
                // And/Or short-circuiting to signal that this result is expected.
                if status != 0 && self.env.errexit_enabled() && !self.errexit_suppressed {
                    return Err(ExecError::ExitRequested(status));
                }
                // Restore the flag to what it was before this statement.
                self.errexit_suppressed = saved;
                Ok(status)
            }
        }
    }

    /// Execute an expression, returning its exit status.
    pub fn execute_expression(&mut self, expr: &Expression, io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        // Apply persistent FDs 0-2 from fd_table (set by `exec` redirects).
        // Clone into locals so the overridden IoContext borrows them.
        let mut fd0 = self.fd_table.get(&0).and_then(|f| f.try_clone().ok());
        let mut fd1 = self.fd_table.get(&1).and_then(|f| f.try_clone().ok());
        let mut fd2 = self.fd_table.get(&2).and_then(|f| f.try_clone().ok());
        let mut persistent_io = IoContext::new(
            match fd0.as_mut() {
                Some(f) => f as &mut dyn std::io::Read,
                None => io.stdin,
            },
            match fd1.as_mut() {
                Some(f) => f as &mut dyn std::io::Write,
                None => io.stdout,
            },
            match fd2.as_mut() {
                Some(f) => f as &mut dyn std::io::Write,
                None => io.stderr,
            },
        );

        self.execute_expression_inner(expr, &mut persistent_io)
    }

    fn execute_expression_inner(&mut self, expr: &Expression, io: &mut IoContext<'_>) -> Result<i32, ExecError> {
        match expr {
            Expression::Command(cmd) => {
                // Try alias expansion before normal execution
                let no_aliases = HashSet::new();
                if let Some(status) = self.try_alias_expansion(cmd, io, &no_aliases)? {
                    return Ok(status);
                }
                self.execute_command(cmd, io)
            }

            Expression::Compound { body, redirects } => self.execute_compound(body, redirects, io),

            Expression::FunctionDef(fndef) => {
                let stored = environment::StoredFunction::from(fndef);
                self.env.set_function(fndef.name.clone(), stored);
                Ok(0)
            }

            Expression::And { left, right } => {
                let saved = self.errexit_suppressed;
                self.errexit_suppressed = true;
                let left_status = self.execute_expression_inner(left, io)?;
                self.errexit_suppressed = saved;
                self.env.set_last_exit_status(left_status);
                if left_status == 0 {
                    self.execute_expression_inner(right, io)
                } else {
                    // Short-circuit: left failed, skip right.  The non-zero
                    // status is an expected control-flow outcome of the &&
                    // chain, so suppress errexit for this result.
                    self.errexit_suppressed = true;
                    Ok(left_status)
                }
            }

            Expression::Or { left, right } => {
                let saved = self.errexit_suppressed;
                self.errexit_suppressed = true;
                let left_status = self.execute_expression_inner(left, io)?;
                self.errexit_suppressed = saved;
                self.env.set_last_exit_status(left_status);
                if left_status != 0 {
                    self.execute_expression_inner(right, io)
                } else {
                    Ok(0)
                }
            }

            Expression::Pipe { .. } => {
                let stages = pipeline::flatten_pipeline(expr);
                pipeline::execute_pipeline(self, &stages, io)
            }

            Expression::Not(inner) => {
                let saved = self.errexit_suppressed;
                self.errexit_suppressed = true;
                let status = self.execute_expression_inner(inner, io)?;
                self.errexit_suppressed = saved;
                let negated = if status == 0 { 1 } else { 0 };
                Ok(negated)
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
                crate::ast::Atom::BashProcessSubstitution { .. } => Err(ExecError::BadSubstitution(
                    "process substitution not supported in POSIX mode".to_string(),
                )),
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
    fn resolve_cmd_subs_in_word(&mut self, word: &crate::ast::Word) -> Result<crate::ast::Word, ExecError> {
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
                Fragment::BashLocaleQuoted { raw, parts } => {
                    let resolved = self.resolve_cmd_subs_in_fragments(parts)?;
                    result.push(Fragment::BashLocaleQuoted {
                        raw: raw.clone(),
                        parts: resolved,
                    });
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
                        let mut argv: Vec<std::ffi::OsString> = Vec::with_capacity(1 + cmd_args.len());
                        argv.push(cmd_name.into());
                        argv.extend(cmd_args.iter().map(std::ffi::OsString::from));

                        let mut child_cmd = command_ex::CommandEx::new(argv);
                        child_cmd.cwd = Some(self.env.cwd().to_path_buf());
                        child_cmd.env = self
                            .env
                            .exported_vars()
                            .into_iter()
                            .map(|(k, v): (String, String)| (std::ffi::OsString::from(k), std::ffi::OsString::from(v)))
                            .collect();
                        child_cmd.fds.insert(1, command_ex::Fd::Pipe);

                        match child_cmd.spawn() {
                            Ok(mut child) => {
                                if let Some(mut pipe) = child.take_pipe(1) {
                                    use std::io::Read;
                                    let _ = pipe.read_to_end(&mut captured);
                                }
                                let code = child.wait().unwrap_or(128);
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

    /// Try to expand the command via alias substitution.
    ///
    /// Returns `Some(exit_status)` if alias expansion happened and the expanded
    /// command was executed, or `None` if no alias matched.
    fn try_alias_expansion(
        &mut self,
        cmd: &Command,
        io: &mut IoContext<'_>,
        already_expanded: &HashSet<String>,
    ) -> Result<Option<i32>, ExecError> {
        if !self.env.expand_aliases_enabled() || cmd.arguments.is_empty() {
            return Ok(None);
        }

        // Only expand plain unquoted literals — 'hi', "hi", $hi etc. must NOT expand.
        let candidate = match &cmd.arguments[0] {
            crate::ast::Argument::Word(w) if w.parts.len() == 1 => match &w.parts[0] {
                crate::ast::Fragment::Literal(name) => name.clone(),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };

        // Check recursion guard
        if already_expanded.contains(&candidate) {
            return Ok(None);
        }

        // Look up in the snapshot (not the live table)
        let expansion = match self.alias_snapshot.get(&candidate) {
            Some(exp) => exp.clone(),
            None => return Ok(None),
        };

        // Build the full command line: alias expansion + remaining original args.
        let mut remaining_args = &cmd.arguments[1..];

        // Trailing-space rule: if the expansion ends with whitespace, the
        // next word is also subject to alias expansion.
        let mut extra_expansion = String::new();
        if expansion.ends_with(' ') || expansion.ends_with('\t') {
            if let Some(crate::ast::Argument::Word(w)) = remaining_args.first() {
                if let [crate::ast::Fragment::Literal(next_name)] = w.parts.as_slice() {
                    if let Some(next_exp) = self.alias_snapshot.get(next_name.as_str()) {
                        extra_expansion = next_exp.clone();
                        remaining_args = &remaining_args[1..];
                    }
                }
            }
        }

        let mut remaining_source = String::new();
        for arg in remaining_args {
            if !remaining_source.is_empty() {
                remaining_source.push(' ');
            }
            match arg.try_to_static_string() {
                Some(s) => remaining_source.push_str(&s),
                None => {
                    let fields = self.expand_argument(arg)?;
                    remaining_source.push_str(&fields.join(" "));
                }
            }
        }

        let mut full_line = expansion.clone();
        if !extra_expansion.is_empty() {
            full_line.push_str(&extra_expansion);
        }
        if !remaining_source.is_empty() {
            if !full_line.ends_with(' ') && !full_line.ends_with('\t') {
                full_line.push(' ');
            }
            full_line.push_str(&remaining_source);
        }

        // Re-parse the expanded line using the executor's options
        let program = match crate::parse_with_options(&full_line, self.options.clone()) {
            Ok(prog) => prog,
            Err(_) => return Ok(None), // parse failure → treat as no expansion
        };

        // Update recursion guard
        let mut expanded = already_expanded.clone();
        expanded.insert(candidate.clone());

        // If the alias expansion ends with whitespace, the next word should
        // also be subject to alias expansion.  We handle this by recursing
        // through the re-parsed program which will naturally hit
        // try_alias_expansion again for each command.

        // Execute the re-parsed program
        let mut status = 0;
        for line in &program.lines {
            for stmt in line {
                status = self.execute_expression_with_alias_guard(&stmt.expression, io, &expanded)?;
                self.env.set_last_exit_status(status);
            }
        }
        Ok(Some(status))
    }

    /// Execute an expression with an alias recursion guard.
    fn execute_expression_with_alias_guard(
        &mut self,
        expr: &Expression,
        io: &mut IoContext<'_>,
        already_expanded: &HashSet<String>,
    ) -> Result<i32, ExecError> {
        match expr {
            Expression::Command(cmd) => {
                // Try alias expansion first
                if let Some(status) = self.try_alias_expansion(cmd, io, already_expanded)? {
                    return Ok(status);
                }
                // No alias match — execute normally
                self.execute_command(cmd, io)
            }
            // For non-Command expressions, delegate normally
            _ => self.execute_expression_inner(expr, io),
        }
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

        // Xtrace: print expanded command to stderr before dispatch.
        if self.env.xtrace_enabled() && !expanded_args.is_empty() {
            let ps4 = self.env.get_var("PS4").unwrap_or("+ ").to_string();
            let _ = writeln!(io.stderr, "{}{}", ps4, expanded_args.join(" "));
        }

        // If no command name, just process assignments
        if expanded_args.is_empty() {
            for assignment in &cmd.assignments {
                self.execute_assignment(assignment)?;
            }
            return Ok(0);
        }

        // Update $_ to the last argument of this simple command.
        let last_arg = expanded_args.last().unwrap().clone();
        self.env.set_last_arg(&last_arg);

        let cmd_name = &expanded_args[0];
        let cmd_args = &expanded_args[1..];

        // Check for special builtins (need Executor access, not just Environment).
        match cmd_name.as_str() {
            "eval" => {
                let saved = self.apply_prefix_assignments(&cmd.assignments)?;
                let mut cmd_io = active.apply_to_io(io);
                let result = self.builtin_eval(cmd_args, &mut cmd_io);
                self.restore_prefix_assignments(saved);
                return result;
            }
            "source" | "." => {
                let saved = self.apply_prefix_assignments(&cmd.assignments)?;
                let mut cmd_io = active.apply_to_io(io);
                let result = self.builtin_source(cmd_args, &mut cmd_io);
                self.restore_prefix_assignments(saved);
                return result;
            }
            "exec" => {
                let saved = self.apply_prefix_assignments(&cmd.assignments)?;
                if cmd_args.is_empty() {
                    // Redirect-only mode: adopt redirects permanently.
                    self.adopt_redirects(active);
                    self.restore_prefix_assignments(saved);
                    return Ok(0);
                }
                let result = self.builtin_exec(cmd_args, &mut active, io);
                self.restore_prefix_assignments(saved);
                return result;
            }
            _ => {}
        }

        // Check for functions first
        if let Some(func) = self.env.get_function(cmd_name).cloned() {
            let saved = self.apply_prefix_assignments(&cmd.assignments)?;
            let call_info = environment::CallInfo {
                function_name: cmd_name.to_string(),
                ..Default::default()
            };
            self.env.push_scope_with_info(cmd_args.to_vec(), call_info);

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

        // Check if this builtin is enabled in the current dialect.
        // Bash-only builtins are skipped when their feature flag is off,
        // falling through to external command lookup (command-not-found).
        let is_active_builtin = if builtins::is_builtin(cmd_name) {
            match cmd_name.as_str() {
                "declare" | "typeset" => self.options.declare_builtin,
                "shopt" => self.options.shopt_builtin,
                "local" => self.options.local_builtin,
                _ => true, // All other builtins are always available
            }
        } else {
            false
        };

        if is_active_builtin {
            let saved = self.apply_prefix_assignments(&cmd.assignments)?;

            let cmd_io = active.apply_to_io(io);
            let mut stdout_buf: Vec<u8> = Vec::new();
            let mut stderr_buf: Vec<u8> = Vec::new();

            // For declare-family builtins, synthesize operands from array
            // assignments so `declare -A m=([foo]=1)` creates `m` as assoc.
            let mut extra_args: Vec<String> = Vec::new();
            for assignment in &cmd.assignments {
                if matches!(assignment.value, crate::ast::AssignmentValue::BashArray(_)) {
                    extra_args.push(assignment.name.clone());
                }
            }
            let all_args: Vec<String> = cmd_args.iter().cloned().chain(extra_args).collect();

            let result = builtins::run_builtin(
                cmd_name,
                &all_args,
                &mut self.env,
                cmd_io.stdin,
                &mut stdout_buf,
                &mut stderr_buf,
            );

            // Execute array assignments AFTER the builtin runs, so declare -A
            // has created the associative array before we set subscripted elements.
            for assignment in &cmd.assignments {
                if matches!(assignment.value, crate::ast::AssignmentValue::BashArray(_)) {
                    self.execute_assignment(assignment)?;
                }
            }

            // Write captured output to the (possibly redirected) io context
            if !stdout_buf.is_empty() {
                cmd_io.stdout.write_all(&stdout_buf).map_err(ExecError::Io)?;
            }
            if !stderr_buf.is_empty() {
                cmd_io.stderr.write_all(&stderr_buf).map_err(ExecError::Io)?;
            }

            self.restore_prefix_assignments(saved);

            return result;
        }

        // External command — pass ActiveRedirects for Stdio setup;
        // `io` is used only for error messages (not redirected).
        self.execute_external(cmd_name, cmd_args, &cmd.assignments, &mut active, io)
    }

    /// Execute a full assignment (scalar, indexed, or array).
    pub(crate) fn execute_assignment(&mut self, assignment: &crate::ast::Assignment) -> Result<(), ExecError> {
        if let Some(ref subscript) = assignment.index {
            // Indexed or associative assignment: name[subscript]=value
            let value = self.expand_word(assignment.value.as_scalar())?;
            if self.env.is_assoc_array(&assignment.name) {
                self.env.set_assoc_element(&assignment.name, subscript, &value)?;
            } else {
                let index: usize = subscript.parse().unwrap_or(0);
                self.env.set_array_element(&assignment.name, index, &value)?;
            }
        } else {
            match &assignment.value {
                crate::ast::AssignmentValue::Scalar(word) => {
                    let value = self.expand_word(word)?;
                    if self.env.has_integer_attr(&assignment.name) {
                        let arith_value = match crate::parser::arith_expr::parse_arith_expr(&value) {
                            Ok(expr) => arithmetic::evaluate_arith_expr(&expr, &mut self.env)?,
                            Err(_) => 0,
                        };
                        self.env.set_var(&assignment.name, &arith_value.to_string())?;
                    } else {
                        self.env.set_var(&assignment.name, &value)?;
                    }
                }
                crate::ast::AssignmentValue::BashArray(elems) => {
                    let mut plain_elements = Vec::new();
                    for elem in elems {
                        match elem {
                            crate::ast::ArrayElement::Plain(word) => {
                                plain_elements.push(self.expand_word(word)?);
                            }
                            crate::ast::ArrayElement::Subscripted { index, value } => {
                                // Flush any accumulated plain elements first
                                if !plain_elements.is_empty() {
                                    self.env
                                        .set_array(&assignment.name, std::mem::take(&mut plain_elements))?;
                                }
                                let val = self.expand_word(value)?;
                                if self.env.is_assoc_array(&assignment.name) {
                                    self.env.set_assoc_element(&assignment.name, index, &val)?;
                                } else {
                                    let idx: usize = index.parse().unwrap_or(0);
                                    self.env.set_array_element(&assignment.name, idx, &val)?;
                                }
                            }
                        }
                    }
                    if !plain_elements.is_empty() {
                        self.env.set_array(&assignment.name, plain_elements)?;
                    }
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
    ///
    /// POSIX requires command-prefix assignments to be exported for the
    /// duration of the command, so each variable is marked exported here.
    /// The saved tuple captures (name, old_value, was_exported) for restore.
    fn apply_prefix_assignments(
        &mut self,
        assignments: &[crate::ast::Assignment],
    ) -> Result<Vec<(String, Option<String>, bool)>, ExecError> {
        let mut saved = Vec::new();
        for assignment in assignments {
            // Skip array assignments — they're handled separately by execute_assignment.
            if !matches!(assignment.value, crate::ast::AssignmentValue::Scalar(_)) {
                continue;
            }
            let old_val = self.env.get_var(&assignment.name).map(|s| s.to_string());
            let old_exported = self.env.is_exported(&assignment.name);
            let value = self.expand_scalar_assignment(assignment)?;
            self.env.set_var(&assignment.name, &value)?;
            self.env.export_var(&assignment.name);
            saved.push((assignment.name.clone(), old_val, old_exported));
        }
        Ok(saved)
    }

    /// Restore prefix assignments from saved values.
    fn restore_prefix_assignments(&mut self, saved: Vec<(String, Option<String>, bool)>) {
        for (name, old_val, old_exported) in saved {
            match old_val {
                Some(val) => {
                    let _ = self.env.set_var(&name, &val);
                    if !old_exported {
                        self.env.unexport_var(&name);
                    }
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
    let program = crate::parse(input).map_err(|e| ExecError::BadSubstitution(format!("parse error: {}", e)))?;
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
