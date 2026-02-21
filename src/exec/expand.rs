use crate::ast::{Argument, Atom, Fragment, ParameterExpansion, Word};
use crate::exec::environment::Environment;
use crate::exec::error::ExecError;

/// Expand a `Word` AST node into a single string.
///
/// This performs the POSIX word expansion steps (in order):
/// 1. Tilde expansion
/// 2. Parameter expansion
/// 3. Command substitution (pre-resolved by Executor before calling this)
/// 4. Arithmetic expansion (pre-resolved by Executor before calling this)
/// 5. Field splitting (done at a higher level)
/// 6. Pathname expansion / globbing (done at a higher level)
/// 7. Quote removal
pub fn expand_word(word: &Word, env: &mut Environment) -> Result<String, ExecError> {
    let mut result = String::new();
    for fragment in &word.parts {
        expand_fragment(fragment, env, &mut result)?;
    }
    Ok(result)
}

/// Expand a word into potentially multiple fields.
///
/// Currently returns a single-element vec. Field splitting and glob expansion
/// will be added in later steps.
pub fn expand_word_to_fields(word: &Word, env: &mut Environment) -> Result<Vec<String>, ExecError> {
    let s = expand_word(word, env)?;
    if s.is_empty() {
        Ok(vec![s])
    } else {
        Ok(vec![s])
    }
}

/// Expand an `Argument` into fields.
pub fn expand_argument(arg: &Argument, env: &mut Environment) -> Result<Vec<String>, ExecError> {
    match arg {
        Argument::Word(word) => expand_word_to_fields(word, env),
        Argument::Atom(atom) => match atom {
            Atom::BashProcessSubstitution { .. } => Err(ExecError::BadSubstitution(
                "process substitution not supported in POSIX mode".to_string(),
            )),
        },
    }
}

/// Expand a single fragment, appending to `out`.
fn expand_fragment(
    fragment: &Fragment,
    env: &mut Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    match fragment {
        Fragment::Literal(s) => {
            out.push_str(s);
        }
        Fragment::SingleQuoted(s) => {
            out.push_str(s);
        }
        Fragment::DoubleQuoted(parts) => {
            for part in parts {
                expand_fragment_in_double_quotes(part, env, out)?;
            }
        }
        Fragment::TildePrefix(user) => {
            expand_tilde(user, env, out);
        }
        Fragment::Parameter(param) => {
            expand_parameter(param, env, out)?;
        }
        Fragment::CommandSubstitution(_) => {
            return Err(ExecError::UnsupportedFeature(
                "unresolved command substitution".to_string(),
            ));
        }
        Fragment::ArithmeticExpansion(_) => {
            return Err(ExecError::UnsupportedFeature(
                "arithmetic expansion $((expr))".to_string(),
            ));
        }
        Fragment::Glob(_) => {
            match fragment {
                Fragment::Glob(crate::ast::GlobChar::Star) => out.push('*'),
                Fragment::Glob(crate::ast::GlobChar::Question) => out.push('?'),
                Fragment::Glob(crate::ast::GlobChar::BracketOpen) => out.push('['),
                _ => unreachable!(),
            }
        }
        Fragment::BashAnsiCQuoted(s) => {
            out.push_str(s);
        }
        Fragment::BashLocaleQuoted(parts) => {
            for part in parts {
                expand_fragment_in_double_quotes(part, env, out)?;
            }
        }
        Fragment::BashExtGlob { .. } => {
            return Err(ExecError::UnsupportedFeature(
                "bash extended glob".to_string(),
            ));
        }
        Fragment::BashBraceExpansion(_) => {
            return Err(ExecError::UnsupportedFeature(
                "bash brace expansion".to_string(),
            ));
        }
    }
    Ok(())
}

/// Expand a fragment inside double quotes (no field splitting, no glob).
fn expand_fragment_in_double_quotes(
    fragment: &Fragment,
    env: &mut Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    match fragment {
        Fragment::Literal(s) => out.push_str(s),
        Fragment::Parameter(param) => expand_parameter(param, env, out)?,
        Fragment::CommandSubstitution(_) => {
            return Err(ExecError::UnsupportedFeature(
                "unresolved command substitution".to_string(),
            ));
        }
        Fragment::ArithmeticExpansion(_) => {
            return Err(ExecError::UnsupportedFeature(
                "arithmetic expansion $((expr))".to_string(),
            ));
        }
        Fragment::SingleQuoted(s) => out.push_str(s),
        Fragment::Glob(g) => {
            match g {
                crate::ast::GlobChar::Star => out.push('*'),
                crate::ast::GlobChar::Question => out.push('?'),
                crate::ast::GlobChar::BracketOpen => out.push('['),
            }
        }
        Fragment::DoubleQuoted(parts) => {
            for part in parts {
                expand_fragment_in_double_quotes(part, env, out)?;
            }
        }
        Fragment::TildePrefix(user) => {
            out.push('~');
            out.push_str(user);
        }
        Fragment::BashAnsiCQuoted(s) => out.push_str(s),
        Fragment::BashLocaleQuoted(parts) => {
            for part in parts {
                expand_fragment_in_double_quotes(part, env, out)?;
            }
        }
        Fragment::BashExtGlob { .. } => {
            return Err(ExecError::UnsupportedFeature(
                "bash extended glob".to_string(),
            ));
        }
        Fragment::BashBraceExpansion(_) => {
            return Err(ExecError::UnsupportedFeature(
                "bash brace expansion".to_string(),
            ));
        }
    }
    Ok(())
}

/// Expand a tilde prefix.
fn expand_tilde(user: &str, env: &mut Environment, out: &mut String) {
    if user.is_empty() {
        if let Some(home) = env.get_var("HOME") {
            out.push_str(home);
        } else {
            out.push('~');
        }
    } else {
        // `~user` → look up user's home directory.
        // TODO: use getpwnam for ~user expansion
        out.push('~');
        out.push_str(user);
    }
}

/// Expand a parameter expansion.
fn expand_parameter(
    param: &ParameterExpansion,
    env: &mut Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    match param {
        ParameterExpansion::Simple(name) => {
            if let Some(val) = env.get_special(name) {
                out.push_str(&val);
            } else if let Some(val) = env.get_var(name) {
                out.push_str(val);
            }
        }
        ParameterExpansion::Complex {
            name,
            operator,
            argument,
        } => {
            expand_complex_parameter(name, operator.as_ref(), argument.as_deref(), env, out)?;
        }
    }
    Ok(())
}

/// Expand a complex parameter expansion like `${var:-default}`.
fn expand_complex_parameter(
    name: &str,
    operator: Option<&crate::ast::ParamOp>,
    argument: Option<&Word>,
    env: &mut Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    use crate::ast::ParamOp;

    let value = env
        .get_special(name)
        .or_else(|| env.get_var(name).map(|s| s.to_string()));

    match operator {
        None => {
            if let Some(val) = value {
                out.push_str(&val);
            }
        }
        Some(ParamOp::Length) => {
            let len = value.as_deref().unwrap_or("").len();
            out.push_str(&len.to_string());
        }
        Some(ParamOp::Default) => {
            match value.as_deref() {
                Some(v) if !v.is_empty() => out.push_str(v),
                _ => {
                    if let Some(arg) = argument {
                        let expanded = expand_word(arg, env)?;
                        out.push_str(&expanded);
                    }
                }
            }
        }
        Some(ParamOp::DefaultAssign) => {
            match value.as_deref() {
                Some(v) if !v.is_empty() => out.push_str(v),
                _ => {
                    let expanded = if let Some(arg) = argument {
                        expand_word(arg, env)?
                    } else {
                        String::new()
                    };
                    env.set_var(name, &expanded)?;
                    out.push_str(&expanded);
                }
            }
        }
        Some(ParamOp::Error) => {
            match value.as_deref() {
                Some(v) if !v.is_empty() => out.push_str(v),
                _ => {
                    let msg = if let Some(arg) = argument {
                        expand_word(arg, env)?
                    } else {
                        format!("{}: parameter null or not set", name)
                    };
                    return Err(ExecError::BadSubstitution(msg));
                }
            }
        }
        Some(ParamOp::Alternative) => {
            match value.as_deref() {
                Some(v) if !v.is_empty() => {
                    if let Some(arg) = argument {
                        let expanded = expand_word(arg, env)?;
                        out.push_str(&expanded);
                    }
                }
                _ => {}
            }
        }
        Some(ParamOp::TrimSmallPrefix) => {
            let val = value.as_deref().unwrap_or("");
            let pat = if let Some(arg) = argument {
                expand_word(arg, env)?
            } else {
                String::new()
            };
            out.push_str(super::pattern::trim_smallest_prefix(val, &pat));
        }
        Some(ParamOp::TrimLargePrefix) => {
            let val = value.as_deref().unwrap_or("");
            let pat = if let Some(arg) = argument {
                expand_word(arg, env)?
            } else {
                String::new()
            };
            out.push_str(super::pattern::trim_largest_prefix(val, &pat));
        }
        Some(ParamOp::TrimSmallSuffix) => {
            let val = value.as_deref().unwrap_or("");
            let pat = if let Some(arg) = argument {
                expand_word(arg, env)?
            } else {
                String::new()
            };
            out.push_str(super::pattern::trim_smallest_suffix(val, &pat));
        }
        Some(ParamOp::TrimLargeSuffix) => {
            let val = value.as_deref().unwrap_or("");
            let pat = if let Some(arg) = argument {
                expand_word(arg, env)?
            } else {
                String::new()
            };
            out.push_str(super::pattern::trim_largest_suffix(val, &pat));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "expand_tests.rs"]
mod tests;
