use crate::ast::{Argument, Atom, Fragment, ParameterExpansion, Word};
use crate::exec::environment::Environment;
use crate::exec::error::ExecError;

/// Expand a `Word` AST node into a single string.
///
/// This performs the POSIX word expansion steps (in order):
/// 1. Tilde expansion
/// 2. Parameter expansion
/// 3. Command substitution (must be pre-resolved before calling this)
/// 4. Arithmetic expansion (TODO)
/// 5. Field splitting (done at a higher level)
/// 6. Pathname expansion / globbing (done at a higher level)
/// 7. Quote removal
pub fn expand_word(word: &Word, env: &Environment) -> Result<String, ExecError> {
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
pub fn expand_word_to_fields(word: &Word, env: &Environment) -> Result<Vec<String>, ExecError> {
    let s = expand_word(word, env)?;
    if s.is_empty() {
        Ok(vec![s])
    } else {
        Ok(vec![s])
    }
}

/// Expand an `Argument` into fields.
pub fn expand_argument(arg: &Argument, env: &Environment) -> Result<Vec<String>, ExecError> {
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
    env: &Environment,
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
        Fragment::CommandSubstitution(_stmts) => {
            // Command substitutions should be pre-resolved by Executor::resolve_cmd_subs()
            // before reaching expand. If we get here, the substitution was not resolved.
            // Expand to empty string (matches unresolved behavior).
        }
        Fragment::ArithmeticExpansion(_expr) => {
            // TODO: implement arithmetic expansion
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
        Fragment::BashExtGlob { .. } | Fragment::BashBraceExpansion(_) => {
            // TODO: Bash-specific expansions
        }
    }
    Ok(())
}

/// Expand a fragment inside double quotes (no field splitting, no glob).
fn expand_fragment_in_double_quotes(
    fragment: &Fragment,
    env: &Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    match fragment {
        Fragment::Literal(s) => out.push_str(s),
        Fragment::Parameter(param) => expand_parameter(param, env, out)?,
        Fragment::CommandSubstitution(_) => {
            // Pre-resolved by Executor, or expand to empty.
        }
        Fragment::ArithmeticExpansion(_) => {
            // TODO: arithmetic expansion
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
        Fragment::BashExtGlob { .. } | Fragment::BashBraceExpansion(_) => {}
    }
    Ok(())
}

/// Expand a tilde prefix.
fn expand_tilde(user: &str, env: &Environment, out: &mut String) {
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
    env: &Environment,
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
    env: &Environment,
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
                    if let Some(arg) = argument {
                        let expanded = expand_word(arg, env)?;
                        out.push_str(&expanded);
                        // TODO: handle ${var:=word} assignment through executor
                    }
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
        Some(ParamOp::TrimSmallSuffix) => {
            // TODO: implement pattern trimming
            if let Some(val) = value {
                out.push_str(&val);
            }
        }
        Some(ParamOp::TrimLargeSuffix) => {
            if let Some(val) = value {
                out.push_str(&val);
            }
        }
        Some(ParamOp::TrimSmallPrefix) => {
            if let Some(val) = value {
                out.push_str(&val);
            }
        }
        Some(ParamOp::TrimLargePrefix) => {
            if let Some(val) = value {
                out.push_str(&val);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Fragment, GlobChar, ParamOp, ParameterExpansion, Word};
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0)
    }

    fn make_word(parts: Vec<Fragment>) -> Word {
        Word {
            parts,
            span: dummy_span(),
        }
    }

    #[test]
    fn expand_literal() {
        let env = Environment::new();
        let word = make_word(vec![Fragment::Literal("hello".into())]);
        assert_eq!(expand_word(&word, &env).unwrap(), "hello");
    }

    #[test]
    fn expand_single_quoted() {
        let env = Environment::new();
        let word = make_word(vec![Fragment::SingleQuoted("don't expand $VAR".into())]);
        assert_eq!(expand_word(&word, &env).unwrap(), "don't expand $VAR");
    }

    #[test]
    fn expand_concatenated_fragments() {
        let mut env = Environment::new();
        env.set_var("NAME", "world").unwrap();
        let word = make_word(vec![
            Fragment::Literal("hello_".into()),
            Fragment::Parameter(ParameterExpansion::Simple("NAME".into())),
        ]);
        assert_eq!(expand_word(&word, &env).unwrap(), "hello_world");
    }

    #[test]
    fn expand_double_quoted_with_param() {
        let mut env = Environment::new();
        env.set_var("X", "value").unwrap();
        let word = make_word(vec![Fragment::DoubleQuoted(vec![
            Fragment::Literal("pre_".into()),
            Fragment::Parameter(ParameterExpansion::Simple("X".into())),
            Fragment::Literal("_post".into()),
        ])]);
        assert_eq!(expand_word(&word, &env).unwrap(), "pre_value_post");
    }

    #[test]
    fn expand_tilde_alone() {
        let mut env = Environment::new();
        env.set_var("HOME", "/home/user").unwrap();
        let word = make_word(vec![Fragment::TildePrefix(String::new())]);
        assert_eq!(expand_word(&word, &env).unwrap(), "/home/user");
    }

    #[test]
    fn expand_tilde_no_home() {
        let mut env = Environment::new();
        env.unset_var("HOME").unwrap();
        let word = make_word(vec![Fragment::TildePrefix(String::new())]);
        assert_eq!(expand_word(&word, &env).unwrap(), "~");
    }

    #[test]
    fn expand_unset_variable() {
        let env = Environment::new();
        let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Simple(
            "NONEXISTENT".into(),
        ))]);
        assert_eq!(expand_word(&word, &env).unwrap(), "");
    }

    #[test]
    fn expand_special_param_question_mark() {
        let mut env = Environment::new();
        env.set_last_exit_status(42);
        let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Simple(
            "?".into(),
        ))]);
        assert_eq!(expand_word(&word, &env).unwrap(), "42");
    }

    #[test]
    fn expand_param_default() {
        let env = Environment::new();
        let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "UNSET".into(),
            operator: Some(ParamOp::Default),
            argument: Some(Box::new(make_word(vec![Fragment::Literal(
                "fallback".into(),
            )]))),
        })]);
        assert_eq!(expand_word(&word, &env).unwrap(), "fallback");
    }

    #[test]
    fn expand_param_default_when_set() {
        let mut env = Environment::new();
        env.set_var("SET", "actual").unwrap();
        let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "SET".into(),
            operator: Some(ParamOp::Default),
            argument: Some(Box::new(make_word(vec![Fragment::Literal(
                "fallback".into(),
            )]))),
        })]);
        assert_eq!(expand_word(&word, &env).unwrap(), "actual");
    }

    #[test]
    fn expand_param_error_when_unset() {
        let env = Environment::new();
        let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "MISSING".into(),
            operator: Some(ParamOp::Error),
            argument: Some(Box::new(make_word(vec![Fragment::Literal(
                "var is required".into(),
            )]))),
        })]);
        let err = expand_word(&word, &env).unwrap_err();
        assert!(matches!(err, ExecError::BadSubstitution(_)));
    }

    #[test]
    fn expand_param_alternative() {
        let mut env = Environment::new();
        env.set_var("SET", "anything").unwrap();
        let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "SET".into(),
            operator: Some(ParamOp::Alternative),
            argument: Some(Box::new(make_word(vec![Fragment::Literal("alt".into())]))),
        })]);
        assert_eq!(expand_word(&word, &env).unwrap(), "alt");
    }

    #[test]
    fn expand_param_alternative_when_unset() {
        let env = Environment::new();
        let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "UNSET".into(),
            operator: Some(ParamOp::Alternative),
            argument: Some(Box::new(make_word(vec![Fragment::Literal("alt".into())]))),
        })]);
        assert_eq!(expand_word(&word, &env).unwrap(), "");
    }

    #[test]
    fn expand_param_length() {
        let mut env = Environment::new();
        env.set_var("STR", "hello").unwrap();
        let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "STR".into(),
            operator: Some(ParamOp::Length),
            argument: None,
        })]);
        assert_eq!(expand_word(&word, &env).unwrap(), "5");
    }

    #[test]
    fn expand_glob_literal_outside_quotes() {
        let env = Environment::new();
        let word = make_word(vec![
            Fragment::Glob(GlobChar::Star),
            Fragment::Literal(".txt".into()),
        ]);
        assert_eq!(expand_word(&word, &env).unwrap(), "*.txt");
    }

    #[test]
    fn expand_glob_literal_inside_double_quotes() {
        let env = Environment::new();
        let word = make_word(vec![Fragment::DoubleQuoted(vec![Fragment::Glob(
            GlobChar::Star,
        )])]);
        assert_eq!(expand_word(&word, &env).unwrap(), "*");
    }
}
