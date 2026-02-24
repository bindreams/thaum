//! POSIX word expansion: tilde, parameter, quote removal. Command substitution
//! and arithmetic are pre-resolved by the `Executor` before calling into this
//! module. Field splitting and pathname expansion are not yet implemented.

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
    Ok(vec![s])
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
fn expand_fragment(fragment: &Fragment, env: &mut Environment, out: &mut String) -> Result<(), ExecError> {
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
        Fragment::Glob(_) => match fragment {
            Fragment::Glob(crate::ast::GlobChar::Star) => out.push('*'),
            Fragment::Glob(crate::ast::GlobChar::Question) => out.push('?'),
            Fragment::Glob(crate::ast::GlobChar::BracketOpen) => out.push('['),
            _ => unreachable!(),
        },
        Fragment::BashAnsiCQuoted(s) => {
            out.push_str(s);
        }
        Fragment::BashLocaleQuoted { raw, parts } => {
            expand_locale_quoted(raw, parts, env, out)?;
        }
        Fragment::BashExtGlob { .. } => {
            return Err(ExecError::UnsupportedFeature("bash extended glob".to_string()));
        }
        Fragment::BashBraceExpansion(_) => {
            return Err(ExecError::UnsupportedFeature("bash brace expansion".to_string()));
        }
    }
    Ok(())
}

/// Expand a `$"..."` locale-quoted fragment via gettext lookup.
///
/// Looks up `raw` as a msgid in the current gettext catalog. If a translation
/// is found, re-parses the translated string as double-quoted content and
/// expands the result. If no translation exists, expands the original `parts`.
fn expand_locale_quoted(
    raw: &str,
    parts: &[Fragment],
    env: &mut Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    let translated = super::gettext::translate(raw, env);
    if translated == *raw {
        // No translation — expand original fragments
        for part in parts {
            expand_fragment_in_double_quotes(part, env, out)?;
        }
    } else {
        // Translation found — re-parse as double-quoted content and expand
        let options = crate::dialect::ShellOptions {
            locale_translation: true,
            ..Default::default()
        };
        let mut lexer = crate::lexer::Lexer::new_double_quote_mode(&translated, options);
        let mut translated_parts = Vec::new();
        loop {
            let tok = lexer
                .next_token()
                .map_err(|e| ExecError::BadSubstitution(format!("gettext re-parse: {}", e)))?;
            if tok.token == crate::token::Token::Eof {
                break;
            }
            translated_parts.push(tok);
        }
        // Convert tokens to fragments via a temporary parser-like path.
        // We only need fragment conversion, so we use a minimal approach:
        // re-parse the translated string as a full double-quoted word.
        // Simpler: just build fragments from the spanned tokens directly.
        // Actually, we can use the same approach as the parser's lex_double_quoted_content:
        // create a parser for the translated string and collect the word.
        // But that's heavyweight. Instead, we expand token-level fragments directly.
        for st in translated_parts {
            expand_dq_token_fragment(&st.token, env, out)?;
        }
    }
    Ok(())
}

/// Expand a double-quoted-context token directly, without building a Fragment AST node.
fn expand_dq_token_fragment(
    token: &crate::token::Token,
    env: &mut Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    use crate::token::Token;
    match token {
        Token::Literal(s) => {
            out.push_str(&de_escape_literal(s));
        }
        Token::SimpleParam(name) => {
            expand_parameter(&ParameterExpansion::Simple(name.clone()), env, out)?;
        }
        Token::BraceParam(raw) => {
            let expansion = crate::word::parse_brace_param_content(raw, true, true);
            expand_parameter(&expansion, env, out)?;
        }
        Token::CommandSub(_) | Token::BacktickSub(_) => {
            return Err(ExecError::UnsupportedFeature(
                "unresolved command substitution in gettext translation".to_string(),
            ));
        }
        Token::ArithSub(_) => {
            return Err(ExecError::UnsupportedFeature(
                "arithmetic expansion in gettext translation".to_string(),
            ));
        }
        _ => {
            // Other tokens in double-quote mode are unlikely but pass through
        }
    }
    Ok(())
}

/// Remove backslash escaping from a raw literal: `\\c` becomes `c`.
fn de_escape_literal(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                result.push(next);
            }
        } else {
            result.push(c);
        }
    }
    result
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
        Fragment::Glob(g) => match g {
            crate::ast::GlobChar::Star => out.push('*'),
            crate::ast::GlobChar::Question => out.push('?'),
            crate::ast::GlobChar::BracketOpen => out.push('['),
        },
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
        Fragment::BashLocaleQuoted { raw, parts } => {
            expand_locale_quoted(raw, parts, env, out)?;
        }
        Fragment::BashExtGlob { .. } => {
            return Err(ExecError::UnsupportedFeature("bash extended glob".to_string()));
        }
        Fragment::BashBraceExpansion(_) => {
            return Err(ExecError::UnsupportedFeature("bash brace expansion".to_string()));
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

/// Parse an array subscript from a parameter name like `"a[0]"` or `"a[@]"`.
///
/// Returns `Some((base_name, subscript))` if the name ends with `[...]`,
/// or `None` for plain variable names.
pub(crate) fn parse_array_subscript(name: &str) -> Option<(&str, &str)> {
    let bracket = name.find('[')?;
    if name.ends_with(']') {
        Some((&name[..bracket], &name[bracket + 1..name.len() - 1]))
    } else {
        None
    }
}

/// Resolve a parameter name to its string value, handling array subscripts.
fn resolve_var(name: &str, env: &Environment) -> Option<String> {
    // Special parameters ($?, $#, $0, $$, etc.) take priority.
    if let Some(val) = env.get_special(name) {
        return Some(val);
    }
    env.resolve_element(name)
}

/// Returns true if a parameter name refers to a special parameter that is
/// always defined (`?`, `#`, `0`, `$`, `!`, `@`, `*`, or a numeric positional).
fn is_special_param(name: &str) -> bool {
    matches!(name, "?" | "#" | "0" | "$" | "!" | "@" | "*") || name.parse::<usize>().is_ok()
}

/// Expand a parameter expansion.
fn expand_parameter(param: &ParameterExpansion, env: &mut Environment, out: &mut String) -> Result<(), ExecError> {
    match param {
        ParameterExpansion::Simple(name) => {
            let value = resolve_var(name, env);
            if value.is_none() && env.nounset_enabled() && !is_special_param(name) {
                return Err(ExecError::UnboundVariable(name.clone()));
            }
            if let Some(val) = value {
                out.push_str(&val);
            }
        }
        ParameterExpansion::Complex {
            name,
            indirect,
            operator,
            argument,
        } => {
            expand_complex_parameter(name, *indirect, operator.as_ref(), argument.as_deref(), env, out)?;
        }
    }
    Ok(())
}

/// Expand a complex parameter expansion like `${var:-default}`.
fn expand_complex_parameter(
    name: &str,
    indirect: bool,
    operator: Option<&crate::ast::ParamOp>,
    argument: Option<&Word>,
    env: &mut Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    use crate::ast::ParamOp;

    // Handle indirect expansion: ${!name...}
    if indirect {
        return expand_indirect(name, operator, argument, env, out);
    }

    let value = resolve_var(name, env);

    match operator {
        None => {
            if value.is_none() && env.nounset_enabled() && !is_special_param(name) {
                return Err(ExecError::UnboundVariable(name.to_string()));
            }
            if let Some(val) = value {
                out.push_str(&val);
            }
        }
        Some(ParamOp::Length) => {
            // ${#a[@]} and ${#a[*]} return the number of array elements.
            // ${#a[0]} returns the string length of element 0.
            // ${#var} returns the string length of the scalar.
            if let Some((base, subscript)) = parse_array_subscript(name) {
                if subscript == "@" || subscript == "*" {
                    let len = env.get_array_length(base);
                    out.push_str(&len.to_string());
                } else {
                    let len = value.as_deref().unwrap_or("").len();
                    out.push_str(&len.to_string());
                }
            } else {
                let len = value.as_deref().unwrap_or("").len();
                out.push_str(&len.to_string());
            }
        }
        Some(ParamOp::Default) => match value.as_deref() {
            Some(v) if !v.is_empty() => out.push_str(v),
            _ => {
                if let Some(arg) = argument {
                    let expanded = expand_word(arg, env)?;
                    out.push_str(&expanded);
                }
            }
        },
        Some(ParamOp::DefaultAssign) => match value.as_deref() {
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
        },
        Some(ParamOp::Error) => match value.as_deref() {
            Some(v) if !v.is_empty() => out.push_str(v),
            _ => {
                let msg = if let Some(arg) = argument {
                    expand_word(arg, env)?
                } else {
                    format!("{}: parameter null or not set", name)
                };
                return Err(ExecError::BadSubstitution(msg));
            }
        },
        Some(ParamOp::Alternative) => match value.as_deref() {
            Some(v) if !v.is_empty() => {
                if let Some(arg) = argument {
                    let expanded = expand_word(arg, env)?;
                    out.push_str(&expanded);
                }
            }
            _ => {}
        },
        Some(ParamOp::TrimSmallPrefix) => {
            let val = value.as_deref().unwrap_or("");
            let pat = if let Some(arg) = argument {
                expand_word(arg, env)?
            } else {
                String::new()
            };
            let locale = super::locale::ctype_locale(env);
            out.push_str(super::pattern::trim_smallest_prefix(val, &pat, &locale));
        }
        Some(ParamOp::TrimLargePrefix) => {
            let val = value.as_deref().unwrap_or("");
            let pat = if let Some(arg) = argument {
                expand_word(arg, env)?
            } else {
                String::new()
            };
            let locale = super::locale::ctype_locale(env);
            out.push_str(super::pattern::trim_largest_prefix(val, &pat, &locale));
        }
        Some(ParamOp::TrimSmallSuffix) => {
            let val = value.as_deref().unwrap_or("");
            let pat = if let Some(arg) = argument {
                expand_word(arg, env)?
            } else {
                String::new()
            };
            let locale = super::locale::ctype_locale(env);
            out.push_str(super::pattern::trim_smallest_suffix(val, &pat, &locale));
        }
        Some(ParamOp::TrimLargeSuffix) => {
            let val = value.as_deref().unwrap_or("");
            let pat = if let Some(arg) = argument {
                expand_word(arg, env)?
            } else {
                String::new()
            };
            let locale = super::locale::ctype_locale(env);
            out.push_str(super::pattern::trim_largest_suffix(val, &pat, &locale));
        }
        Some(ParamOp::UpperFirst) => {
            let val = value.as_deref().unwrap_or("");
            let locale = super::locale::ctype_locale(env);
            out.push_str(&super::locale::capitalize(val, &locale));
        }
        Some(ParamOp::UpperAll) => {
            let val = value.as_deref().unwrap_or("");
            let locale = super::locale::ctype_locale(env);
            out.push_str(&super::locale::to_uppercase(val, &locale));
        }
        Some(ParamOp::LowerFirst) => {
            let val = value.as_deref().unwrap_or("");
            let locale = super::locale::ctype_locale(env);
            out.push_str(&super::locale::uncapitalize(val, &locale));
        }
        Some(ParamOp::LowerAll) => {
            let val = value.as_deref().unwrap_or("");
            let locale = super::locale::ctype_locale(env);
            out.push_str(&super::locale::to_lowercase(val, &locale));
        }
        Some(ParamOp::TransformQuote) => {
            let val = value.as_deref().unwrap_or("");
            let quoted = format!("'{}'", val.replace('\'', "'\\''"));
            out.push_str(&quoted);
        }
        Some(ParamOp::TransformAttributes) => {
            let flags = env.get_var_attributes(name);
            out.push_str(&flags);
        }
        Some(ParamOp::TransformAssignment) => {
            let val = value.as_deref().unwrap_or("");
            let flags = env.get_var_attributes(name);
            // Strip array subscript from name for display: "a[@]" -> "a"
            let base_name = if let Some((base, _)) = parse_array_subscript(name) {
                base
            } else {
                name
            };
            if flags.is_empty() {
                out.push_str(&format!("{}='{}'", base_name, val));
            } else {
                out.push_str(&format!("declare -{} {}='{}'", flags, base_name, val));
            }
        }
        Some(ParamOp::TransformEscape) => {
            // ANSI-C escape expansion (simplified — pass through for now)
            let val = value.as_deref().unwrap_or("");
            out.push_str(val);
        }
        Some(ParamOp::TransformPrompt) => {
            // Prompt-string expansion (simplified — pass through for now)
            let val = value.as_deref().unwrap_or("");
            out.push_str(val);
        }
        Some(ParamOp::TransformLower) => {
            let val = value.as_deref().unwrap_or("");
            let locale = super::locale::ctype_locale(env);
            out.push_str(&super::locale::to_lowercase(val, &locale));
        }
        Some(ParamOp::TransformUpper) => {
            let val = value.as_deref().unwrap_or("");
            let locale = super::locale::ctype_locale(env);
            out.push_str(&super::locale::to_uppercase(val, &locale));
        }
        Some(ParamOp::TransformCapitalize) => {
            let val = value.as_deref().unwrap_or("");
            let locale = super::locale::ctype_locale(env);
            out.push_str(&super::locale::capitalize(val, &locale));
        }
        Some(ParamOp::TransformKeyValue) | Some(ParamOp::TransformKeys) => {
            // TODO: key=value formatting for arrays
            let val = value.as_deref().unwrap_or("");
            out.push_str(val);
        }
    }
    Ok(())
}

/// Handle indirect expansion: `${!name}` or `${!name[@]}`.
///
/// - `${!name[@]}` / `${!name[*]}` — list the keys of the array named `name`.
/// - `${!name}` — resolve the value of `$name`, use that string as a variable
///   name, and expand that variable.
fn expand_indirect(
    name: &str,
    operator: Option<&crate::ast::ParamOp>,
    _argument: Option<&Word>,
    env: &mut Environment,
    out: &mut String,
) -> Result<(), ExecError> {
    // Check if name contains an array subscript like "a[@]" or "a[*]"
    if let Some((base, subscript)) = parse_array_subscript(name) {
        if subscript == "@" || subscript == "*" {
            // ${!name[@]} — list array keys
            if let Some(keys) = env.get_array_keys(base) {
                out.push_str(&keys.join(" "));
            }
            return Ok(());
        }
    }

    // ${!name} — indirect variable reference.
    // Get the value of $name, use THAT as the variable name.
    let target_name = match resolve_var(name, env) {
        Some(val) => val,
        None => return Ok(()),
    };

    // Now resolve the target variable, applying any operator
    let target_value = resolve_var(&target_name, env);

    match operator {
        None => {
            if let Some(val) = target_value {
                out.push_str(&val);
            }
        }
        _ => {
            // For other operators applied to an indirect ref, recurse with the
            // resolved target name and indirect=false.
            return expand_complex_parameter(&target_name, false, operator, _argument, env, out);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "expand_tests.rs"]
mod tests;
