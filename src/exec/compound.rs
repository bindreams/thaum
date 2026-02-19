use crate::ast::{CompoundCommand, Redirect};
use crate::exec::error::ExecError;
use crate::exec::pattern::shell_pattern_match;
use crate::exec::Executor;

impl Executor {
    /// Execute a compound command with optional redirections.
    pub(super) fn execute_compound(
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
