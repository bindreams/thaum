pub mod builtins;
pub mod environment;
pub mod error;
pub mod expand;
pub mod pipeline;

pub use environment::Environment;
pub use error::ExecError;

use std::io::Write;
use std::process::Stdio;

use crate::ast::{
    Command, CompoundCommand, Expression, ExecutionMode, Program, Redirect, RedirectKind,
    Statement,
};

/// The shell executor.
///
/// Takes a parsed AST and executes it, maintaining shell state (variables,
/// functions, CWD, exit status) in an `Environment`.
pub struct Executor {
    env: Environment,
}

impl Executor {
    /// Create a new executor with default environment.
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.inherit_from_process();
        Executor { env }
    }

    /// Create an executor with a specific environment.
    pub fn with_env(env: Environment) -> Self {
        Executor { env }
    }

    /// Get a mutable reference to the environment.
    pub fn env_mut(&mut self) -> &mut Environment {
        &mut self.env
    }

    /// Get a reference to the environment.
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Execute a parsed program. Returns the exit status of the last command.
    pub fn execute(&mut self, program: &Program) -> Result<i32, ExecError> {
        self.execute_statements(&program.statements)
    }

    /// Execute a list of statements, returning the last exit status.
    pub fn execute_statements(&mut self, stmts: &[Statement]) -> Result<i32, ExecError> {
        let mut status = 0;
        for stmt in stmts {
            status = self.execute_statement(stmt)?;
        }
        Ok(status)
    }

    /// Execute a single statement.
    fn execute_statement(&mut self, stmt: &Statement) -> Result<i32, ExecError> {
        match stmt.mode {
            ExecutionMode::Background => {
                return Err(ExecError::UnsupportedFeature(
                    "background execution (&)".to_string(),
                ));
            }
            ExecutionMode::Sequential | ExecutionMode::Terminated => {
                let status = self.execute_expression(&stmt.expression)?;
                self.env.set_last_exit_status(status);
                Ok(status)
            }
        }
    }

    /// Execute an expression, returning its exit status.
    pub fn execute_expression(&mut self, expr: &Expression) -> Result<i32, ExecError> {
        match expr {
            Expression::Command(cmd) => self.execute_command(cmd),

            Expression::Compound { body, redirects } => {
                self.execute_compound(body, redirects)
            }

            Expression::FunctionDef(fndef) => {
                let stored = environment::StoredFunction::from(fndef);
                self.env.set_function(fndef.name.clone(), stored);
                Ok(0)
            }

            Expression::And { left, right } => {
                let left_status = self.execute_expression(left)?;
                self.env.set_last_exit_status(left_status);
                if left_status == 0 {
                    self.execute_expression(right)
                } else {
                    Ok(left_status)
                }
            }

            Expression::Or { left, right } => {
                let left_status = self.execute_expression(left)?;
                self.env.set_last_exit_status(left_status);
                if left_status != 0 {
                    self.execute_expression(right)
                } else {
                    Ok(0)
                }
            }

            Expression::Pipe { .. } => {
                let stages = pipeline::flatten_pipeline(expr);
                pipeline::execute_pipeline(self, &stages)
            }

            Expression::Not(inner) => {
                let status = self.execute_expression(inner)?;
                Ok(if status == 0 { 1 } else { 0 })
            }
        }
    }

    /// Expand a word, resolving command substitutions first.
    fn expand_word(&mut self, word: &crate::ast::Word) -> Result<String, ExecError> {
        let resolved = self.resolve_cmd_subs_in_word(word)?;
        expand::expand_word(&resolved, &self.env)
    }

    /// Expand an argument, resolving command substitutions first.
    fn expand_argument(
        &mut self,
        arg: &crate::ast::Argument,
    ) -> Result<Vec<String>, ExecError> {
        match arg {
            crate::ast::Argument::Word(word) => {
                let resolved = self.resolve_cmd_subs_in_word(word)?;
                expand::expand_word_to_fields(&resolved, &self.env)
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
    fn expand_word_to_fields(
        &mut self,
        word: &crate::ast::Word,
    ) -> Result<Vec<String>, ExecError> {
        let resolved = self.resolve_cmd_subs_in_word(word)?;
        expand::expand_word_to_fields(&resolved, &self.env)
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
    fn execute_command_substitution(
        &mut self,
        stmts: &[Statement],
    ) -> Result<String, ExecError> {
        let mut captured = Vec::new();

        for stmt in stmts {
            match &stmt.expression {
                Expression::Command(cmd) => {
                    let mut args: Vec<String> = Vec::new();
                    for arg in &cmd.arguments {
                        let fields = expand::expand_argument(arg, &self.env)?;
                        args.extend(fields);
                    }

                    if args.is_empty() {
                        for assignment in &cmd.assignments {
                            let value =
                                expand::expand_word(&assignment.value.as_scalar(), &self.env)?;
                            self.env.set_var(&assignment.name, &value)?;
                        }
                        continue;
                    }

                    let cmd_name = &args[0];
                    let cmd_args = &args[1..];

                    if builtins::is_builtin(cmd_name) {
                        let mut stderr_buf = Vec::new();
                        let status = builtins::run_builtin(
                            cmd_name,
                            cmd_args,
                            &mut self.env,
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
    fn execute_command(&mut self, cmd: &Command) -> Result<i32, ExecError> {
        // Expand arguments
        let mut expanded_args: Vec<String> = Vec::new();
        for arg in &cmd.arguments {
            let fields = self.expand_argument(arg)?;
            expanded_args.extend(fields);
        }

        // If no command name, just process assignments
        if expanded_args.is_empty() {
            for assignment in &cmd.assignments {
                let value = self.expand_word(&assignment.value.as_scalar())?;
                self.env.set_var(&assignment.name, &value)?;
            }
            return Ok(0);
        }

        let cmd_name = &expanded_args[0];
        let cmd_args = &expanded_args[1..];

        // Check for functions first
        if let Some(func) = self.env.get_function(cmd_name).cloned() {
            // Apply prefix assignments temporarily
            let saved = self.apply_prefix_assignments(&cmd.assignments)?;

            // Push new scope with positional params
            self.env.push_scope(cmd_args.to_vec());

            let result = self.execute_compound(&func.body, &func.redirects);

            // Pop scope
            self.env.pop_scope();

            // Restore prefix assignments
            self.restore_prefix_assignments(saved);

            return match result {
                Ok(status) => Ok(status),
                Err(ExecError::ReturnRequested(code)) => Ok(code),
                Err(e) => Err(e),
            };
        }

        // Check for builtins
        if builtins::is_builtin(cmd_name) {
            // Apply prefix assignments temporarily for builtins
            let saved = self.apply_prefix_assignments(&cmd.assignments)?;

            let mut stdout_buf: Vec<u8> = Vec::new();
            let mut stderr_buf: Vec<u8> = Vec::new();

            let result =
                builtins::run_builtin(cmd_name, cmd_args, &mut self.env, &mut stdout_buf, &mut stderr_buf);

            // Write captured output to actual stdout/stderr
            if !stdout_buf.is_empty() {
                std::io::stdout()
                    .write_all(&stdout_buf)
                    .map_err(ExecError::Io)?;
            }
            if !stderr_buf.is_empty() {
                std::io::stderr()
                    .write_all(&stderr_buf)
                    .map_err(ExecError::Io)?;
            }

            // Restore prefix assignments
            self.restore_prefix_assignments(saved);

            return result;
        }

        // External command
        self.execute_external(cmd_name, cmd_args, &cmd.assignments, &cmd.redirects)
    }

    /// Execute an external command via fork/exec.
    fn execute_external(
        &mut self,
        name: &str,
        args: &[String],
        assignments: &[crate::ast::Assignment],
        redirects: &[Redirect],
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
                eprintln!("{}: command not found", name);
                Ok(127)
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("{}: permission denied", name);
                Ok(126)
            }
            Err(e) => Err(ExecError::Io(e)),
        }
    }

    /// Apply prefix assignments temporarily, returning saved values.
    fn apply_prefix_assignments(
        &mut self,
        assignments: &[crate::ast::Assignment],
    ) -> Result<Vec<(String, Option<String>)>, ExecError> {
        let mut saved = Vec::new();
        for assignment in assignments {
            let old = self.env.get_var(&assignment.name).map(|s| s.to_string());
            let value = self.expand_word(&assignment.value.as_scalar())?;
            self.env.set_var(&assignment.name, &value)?;
            saved.push((assignment.name.clone(), old));
        }
        Ok(saved)
    }

    /// Restore prefix assignments from saved values.
    fn restore_prefix_assignments(&mut self, saved: Vec<(String, Option<String>)>) {
        for (name, old_val) in saved {
            match old_val {
                Some(val) => { let _ = self.env.set_var(&name, &val); }
                None => { let _ = self.env.unset_var(&name); }
            }
        }
    }

    /// Apply redirections to a std::process::Command.
    fn apply_redirects_to_command(
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
    fn resolve_path(&self, path: &str) -> std::path::PathBuf {
        let p = std::path::Path::new(path);
        if p.is_relative() {
            self.env.cwd().join(p)
        } else {
            p.to_path_buf()
        }
    }

    /// Execute a compound command with optional redirections.
    fn execute_compound(
        &mut self,
        body: &CompoundCommand,
        redirects: &[Redirect],
    ) -> Result<i32, ExecError> {
        if !redirects.is_empty() {
            return Err(ExecError::UnsupportedFeature(
                "redirections on compound commands".to_string(),
            ));
        }
        match body {
            CompoundCommand::BraceGroup { body, .. } => {
                self.execute_statements(body)
            }

            CompoundCommand::Subshell { .. } => {
                return Err(ExecError::UnsupportedFeature(
                    "subshell (requires fork)".to_string(),
                ));
            }

            CompoundCommand::IfClause {
                condition,
                then_body,
                elifs,
                else_body,
                ..
            } => {
                let cond_status = self.execute_statements(condition)?;
                if cond_status == 0 {
                    return self.execute_statements(then_body);
                }
                for elif in elifs {
                    let elif_status = self.execute_statements(&elif.condition)?;
                    if elif_status == 0 {
                        return self.execute_statements(&elif.body);
                    }
                }
                if let Some(else_body) = else_body {
                    self.execute_statements(else_body)
                } else {
                    Ok(0)
                }
            }

            CompoundCommand::WhileClause {
                condition, body, ..
            } => {
                let mut status = 0;
                loop {
                    let cond_status = self.execute_statements(condition)?;
                    if cond_status != 0 {
                        break;
                    }
                    match self.execute_statements(body) {
                        Ok(s) => status = s,
                        Err(ExecError::BreakRequested(1)) => break,
                        Err(ExecError::BreakRequested(n)) => {
                            return Err(ExecError::BreakRequested(n - 1));
                        }
                        Err(ExecError::ContinueRequested(1)) => continue,
                        Err(ExecError::ContinueRequested(n)) => {
                            return Err(ExecError::ContinueRequested(n - 1));
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(status)
            }

            CompoundCommand::UntilClause {
                condition, body, ..
            } => {
                let mut status = 0;
                loop {
                    let cond_status = self.execute_statements(condition)?;
                    if cond_status == 0 {
                        break;
                    }
                    match self.execute_statements(body) {
                        Ok(s) => status = s,
                        Err(ExecError::BreakRequested(1)) => break,
                        Err(ExecError::BreakRequested(n)) => {
                            return Err(ExecError::BreakRequested(n - 1));
                        }
                        Err(ExecError::ContinueRequested(1)) => continue,
                        Err(ExecError::ContinueRequested(n)) => {
                            return Err(ExecError::ContinueRequested(n - 1));
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(status)
            }

            CompoundCommand::ForClause {
                variable,
                words,
                body,
                ..
            } => {
                let word_list = if let Some(words) = words {
                    let mut list = Vec::new();
                    for word in words {
                        let fields = self.expand_word_to_fields(word)?;
                        list.extend(fields);
                    }
                    list
                } else {
                    // `for var; do ... done` — iterate over positional params
                    self.env.positional_params().to_vec()
                };

                let mut status = 0;
                for value in &word_list {
                    self.env.set_var(variable, value)?;
                    match self.execute_statements(body) {
                        Ok(s) => status = s,
                        Err(ExecError::BreakRequested(1)) => break,
                        Err(ExecError::BreakRequested(n)) => {
                            return Err(ExecError::BreakRequested(n - 1));
                        }
                        Err(ExecError::ContinueRequested(1)) => continue,
                        Err(ExecError::ContinueRequested(n)) => {
                            return Err(ExecError::ContinueRequested(n - 1));
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(status)
            }

            CompoundCommand::CaseClause { word, arms, .. } => {
                let expanded = self.expand_word(word)?;
                let mut status = 0;

                for arm in arms {
                    let mut matched = false;
                    for pattern in &arm.patterns {
                        let pat = self.expand_word(pattern)?;
                        if shell_pattern_match(&expanded, &pat) {
                            matched = true;
                            break;
                        }
                    }
                    if matched {
                        status = self.execute_statements(&arm.body)?;
                        // For POSIX `;;`, break after first match.
                        // Bash `;;&` and `;&` are handled differently (not yet).
                        break;
                    }
                }
                Ok(status)
            }

            CompoundCommand::BashDoubleBracket { .. } => {
                return Err(ExecError::UnsupportedFeature(
                    "bash [[ ]] conditional".to_string(),
                ));
            }
            CompoundCommand::BashArithmeticCommand { .. } => {
                return Err(ExecError::UnsupportedFeature(
                    "bash (( )) arithmetic command".to_string(),
                ));
            }
            CompoundCommand::BashSelectClause { .. } => {
                return Err(ExecError::UnsupportedFeature(
                    "bash select clause".to_string(),
                ));
            }
            CompoundCommand::BashCoproc { .. } => {
                return Err(ExecError::UnsupportedFeature(
                    "bash coproc".to_string(),
                ));
            }
            CompoundCommand::BashArithmeticFor { .. } => {
                return Err(ExecError::UnsupportedFeature(
                    "bash arithmetic for loop".to_string(),
                ));
            }
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple shell pattern matching for `case` arms.
///
/// Supports `*`, `?`, and character classes `[...]`.
/// Does not support extended globs.
fn shell_pattern_match(text: &str, pattern: &str) -> bool {
    let text = text.as_bytes();
    let pattern = pattern.as_bytes();
    match_pattern(text, pattern, 0, 0)
}

fn match_pattern(text: &[u8], pattern: &[u8], mut ti: usize, mut pi: usize) -> bool {
    while pi < pattern.len() {
        if ti < text.len() && pattern[pi] == b'?' {
            ti += 1;
            pi += 1;
        } else if pattern[pi] == b'*' {
            // Skip consecutive stars
            while pi < pattern.len() && pattern[pi] == b'*' {
                pi += 1;
            }
            if pi == pattern.len() {
                return true;
            }
            // Try matching the rest at each position
            for i in ti..=text.len() {
                if match_pattern(text, pattern, i, pi) {
                    return true;
                }
            }
            return false;
        } else if pattern[pi] == b'[' {
            // Character class
            pi += 1;
            let negate = pi < pattern.len() && (pattern[pi] == b'!' || pattern[pi] == b'^');
            if negate {
                pi += 1;
            }

            let mut matched = false;
            let mut first = true;
            while pi < pattern.len() && (first || pattern[pi] != b']') {
                first = false;
                if pi + 2 < pattern.len() && pattern[pi + 1] == b'-' {
                    // Range: [a-z]
                    if ti < text.len() && text[ti] >= pattern[pi] && text[ti] <= pattern[pi + 2] {
                        matched = true;
                    }
                    pi += 3;
                } else {
                    if ti < text.len() && text[ti] == pattern[pi] {
                        matched = true;
                    }
                    pi += 1;
                }
            }
            if pi < pattern.len() && pattern[pi] == b']' {
                pi += 1;
            }
            if negate {
                matched = !matched;
            }
            if !matched || ti >= text.len() {
                return false;
            }
            ti += 1;
        } else if ti < text.len() && pattern[pi] == text[ti] {
            ti += 1;
            pi += 1;
        } else {
            return false;
        }
    }
    ti == text.len()
}

/// Convenience: parse and execute a script string.
pub fn run(input: &str) -> Result<i32, ExecError> {
    let program = crate::parse(input).map_err(|e| {
        ExecError::BadSubstitution(format!("parse error: {}", e))
    })?;
    let mut executor = Executor::new();
    match executor.execute(&program) {
        Ok(status) => Ok(status),
        Err(ExecError::ExitRequested(code)) => Ok(code),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_match_literal() {
        assert!(shell_pattern_match("hello", "hello"));
        assert!(!shell_pattern_match("hello", "world"));
    }

    #[test]
    fn pattern_match_star() {
        assert!(shell_pattern_match("hello", "*"));
        assert!(shell_pattern_match("hello", "hel*"));
        assert!(shell_pattern_match("hello", "*llo"));
        assert!(shell_pattern_match("hello", "h*o"));
        assert!(!shell_pattern_match("hello", "h*x"));
    }

    #[test]
    fn pattern_match_question() {
        assert!(shell_pattern_match("hello", "hell?"));
        assert!(shell_pattern_match("hello", "?ello"));
        assert!(!shell_pattern_match("hello", "hell"));
    }

    #[test]
    fn pattern_match_bracket() {
        assert!(shell_pattern_match("hello", "[h]ello"));
        assert!(shell_pattern_match("hello", "[a-z]ello"));
        assert!(!shell_pattern_match("hello", "[A-Z]ello"));
        assert!(shell_pattern_match("hello", "[!A-Z]ello"));
    }
}
