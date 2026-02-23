//! Execution of compound commands: `if`/`while`/`until`/`for`/`case`/
//! brace-group/subshell/`select`/`[[ ]]`/`(( ))` and C-style `for`.

use crate::ast::{CompoundCommand, Redirect};
use crate::exec::arithmetic;
use crate::exec::error::ExecError;
use crate::exec::io_context::IoContext;
use crate::exec::pattern::shell_pattern_match;
use crate::exec::Executor;

impl Executor {
    /// Execute a compound command with optional redirections.
    pub(super) fn execute_compound(
        &mut self,
        body: &CompoundCommand,
        redirects: &[Redirect],
        io: &mut IoContext<'_>,
    ) -> Result<i32, ExecError> {
        if !redirects.is_empty() {
            return Err(ExecError::UnsupportedFeature(
                "redirections on compound commands".to_string(),
            ));
        }
        match body {
            CompoundCommand::BraceGroup { body, .. } => self.execute_lines(body, io),

            CompoundCommand::Subshell { body, .. } => self.execute_subshell(body, io),

            CompoundCommand::IfClause {
                condition,
                then_body,
                elifs,
                else_body,
                ..
            } => {
                let cond_status = self.execute_lines(condition, io)?;
                if cond_status == 0 {
                    return self.execute_lines(then_body, io);
                }
                for elif in elifs {
                    let elif_status = self.execute_lines(&elif.condition, io)?;
                    if elif_status == 0 {
                        return self.execute_lines(&elif.body, io);
                    }
                }
                if let Some(else_body) = else_body {
                    self.execute_lines(else_body, io)
                } else {
                    Ok(0)
                }
            }

            CompoundCommand::WhileClause { condition, body, .. } => {
                let mut status = 0;
                loop {
                    let cond_status = self.execute_lines(condition, io)?;
                    if cond_status != 0 {
                        break;
                    }
                    match self.execute_lines(body, io) {
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

            CompoundCommand::UntilClause { condition, body, .. } => {
                let mut status = 0;
                loop {
                    let cond_status = self.execute_lines(condition, io)?;
                    if cond_status == 0 {
                        break;
                    }
                    match self.execute_lines(body, io) {
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
                variable, words, body, ..
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
                    match self.execute_lines(body, io) {
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
                        status = self.execute_lines(&arm.body, io)?;
                        // For POSIX `;;`, break after first match.
                        // Bash `;;&` and `;&` are handled differently (not yet).
                        break;
                    }
                }
                Ok(status)
            }

            CompoundCommand::BashDoubleBracket { expression, .. } => {
                let result = super::bash_test::evaluate(expression, self, io)?;
                Ok(if result { 0 } else { 1 })
            }
            CompoundCommand::BashArithmeticCommand { expression, .. } => {
                let value = arithmetic::evaluate_arith_expr(expression, &mut self.env)?;
                // (( )) returns 0 (success) if expression is non-zero,
                // 1 (failure) if expression is zero.
                Ok(if value != 0 { 0 } else { 1 })
            }
            CompoundCommand::BashSelectClause { .. } => {
                Err(ExecError::UnsupportedFeature("bash select clause".to_string()))
            }
            CompoundCommand::BashCoproc { .. } => Err(ExecError::UnsupportedFeature("bash coproc".to_string())),
            CompoundCommand::BashArithmeticFor {
                init,
                condition,
                update,
                body,
                ..
            } => {
                if let Some(init_expr) = init {
                    arithmetic::evaluate_arith_expr(init_expr, &mut self.env)?;
                }

                let mut status = 0;
                loop {
                    // Empty condition means infinite loop
                    if let Some(cond_expr) = condition {
                        let cond = arithmetic::evaluate_arith_expr(cond_expr, &mut self.env)?;
                        if cond == 0 {
                            break;
                        }
                    }

                    let should_update = match self.execute_lines(body, io) {
                        Ok(s) => {
                            status = s;
                            true
                        }
                        Err(ExecError::BreakRequested(1)) => break,
                        Err(ExecError::BreakRequested(n)) => {
                            return Err(ExecError::BreakRequested(n - 1));
                        }
                        // continue still evaluates the update expression
                        Err(ExecError::ContinueRequested(1)) => true,
                        Err(ExecError::ContinueRequested(n)) => {
                            return Err(ExecError::ContinueRequested(n - 1));
                        }
                        Err(e) => return Err(e),
                    };

                    if should_update {
                        if let Some(update_expr) = update {
                            arithmetic::evaluate_arith_expr(update_expr, &mut self.env)?;
                        }
                    }
                }
                Ok(status)
            }
        }
    }
}
