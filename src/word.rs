//! Word expansion parser: breaks a raw word string into structured `Fragment` components.
//!
//! The lexer produces raw word strings that may contain quotes, parameter
//! expansions, command substitutions, etc. This module parses those strings
//! into the structured `Fragment` representation.

mod params;
mod subst;

use crate::ast::*;
use crate::dialect::ParseOptions;
use crate::span::Span;

use params::{is_special_param, parse_brace_expansion, parse_simple_parameter};
use subst::{parse_command_substitution, read_balanced_parens, read_until_matching_paren};

/// Parse a raw word string into an `Argument`.
///
/// Returns `Argument::Atom` for standalone constructs (process substitution),
/// `Argument::Word` for everything else.
pub fn parse_argument(raw: &str, span: Span, options: &ParseOptions) -> Argument {
    // Process substitution: entire word is <(...) or >(...)
    if (raw.starts_with("<(") || raw.starts_with(">(")) && raw.ends_with(')') {
        let direction = if raw.starts_with('<') {
            ProcessDirection::In
        } else {
            ProcessDirection::Out
        };
        let content = &raw[2..raw.len() - 1];
        let stmts = parse_command_substitution(content);
        return Argument::Atom(Atom::BashProcessSubstitution {
            direction,
            body: stmts,
            span,
        });
    }
    Argument::Word(parse_word(raw, span, options))
}

/// Parse a raw word string (as produced by the lexer) into a structured `Word`.
pub fn parse_word(raw: &str, span: Span, options: &ParseOptions) -> Word {
    let parts = parse_fragments(raw, false, options);
    Word { parts, span }
}

/// Parse fragments from a raw string.
/// `in_double_quote` indicates if we're inside a double-quoted context.
fn parse_fragments(raw: &str, in_double_quote: bool, options: &ParseOptions) -> Vec<Fragment> {
    let mut parts = Vec::new();
    let mut chars = raw.chars().peekable();
    let mut literal = String::new();

    // Handle tilde prefix at the start (only outside double quotes)
    if !in_double_quote {
        if let Some(&'~') = chars.peek() {
            chars.next(); // consume ~
            let mut user = String::new();
            while let Some(&ch) = chars.peek() {
                if ch == '/' || ch == ':' || ch == ' ' || ch == '\t' {
                    break;
                }
                // Stop tilde prefix at any special character
                if ch == '$' || ch == '`' || ch == '\'' || ch == '"' || ch == '\\' {
                    break;
                }
                user.push(ch);
                chars.next();
            }
            parts.push(Fragment::TildePrefix(user));
            // If there's nothing left, return
            if chars.peek().is_none() {
                return parts;
            }
        }
    }

    while let Some(&ch) = chars.peek() {
        match ch {
            // Single quote (not inside double quotes)
            '\'' if !in_double_quote => {
                flush_literal(&mut literal, &mut parts);
                chars.next(); // consume opening '
                let mut content = String::new();
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => content.push(c),
                        None => break, // unterminated (lexer already validated)
                    }
                }
                parts.push(Fragment::SingleQuoted(content));
            }

            // Double quote
            '"' if !in_double_quote => {
                flush_literal(&mut literal, &mut parts);
                chars.next(); // consume opening "
                              // Collect the content between quotes as a string, then parse it
                let mut dq_content = String::new();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => {
                            // In double quotes, backslash escapes only $, `, ", \, and newline
                            match chars.peek() {
                                Some(&c)
                                    if c == '$'
                                        || c == '`'
                                        || c == '"'
                                        || c == '\\'
                                        || c == '\n' =>
                                {
                                    dq_content.push('\\');
                                    dq_content.push(c);
                                    chars.next();
                                }
                                _ => {
                                    dq_content.push('\\');
                                }
                            }
                        }
                        Some(c) => dq_content.push(c),
                        None => break,
                    }
                }
                let inner_parts = parse_fragments(&dq_content, true, options);
                parts.push(Fragment::DoubleQuoted(inner_parts));
            }

            // Backslash escape
            '\\' if !in_double_quote => {
                chars.next(); // consume backslash
                if let Some(c) = chars.next() {
                    // Escaped character is literal
                    literal.push(c);
                }
            }

            '\\' if in_double_quote => {
                chars.next(); // consume backslash
                if let Some(&next) = chars.peek() {
                    if next == '$' || next == '`' || next == '"' || next == '\\' || next == '\n' {
                        chars.next();
                        literal.push(next);
                    } else {
                        literal.push('\\');
                    }
                } else {
                    literal.push('\\');
                }
            }

            // Dollar sign — parameter expansion, command substitution, or arithmetic
            '$' => {
                flush_literal(&mut literal, &mut parts);
                chars.next(); // consume $
                match chars.peek() {
                    Some(&'(') => {
                        chars.next(); // consume (
                        if chars.peek() == Some(&'(') {
                            // Arithmetic expansion: $((expr))
                            chars.next(); // consume second (
                            let expr = read_balanced_parens(&mut chars);
                            // Fallback to raw string if arithmetic parsing fails.
                            // The closure is intentional — `expr` is moved into Variable.
                            #[allow(clippy::unnecessary_lazy_evaluations)]
                            let arith = crate::parser::arith_expr::parse_arith_expr(&expr)
                                .unwrap_or_else(|_| ArithExpr::Variable(expr));
                            parts.push(Fragment::ArithmeticExpansion(arith));
                        } else {
                            // Command substitution: $(cmd)
                            let cmd = read_until_matching_paren(&mut chars);
                            let stmts = parse_command_substitution(&cmd);
                            parts.push(Fragment::CommandSubstitution(stmts));
                        }
                    }
                    Some(&'{') => {
                        chars.next(); // consume {
                        let expansion = parse_brace_expansion(&mut chars);
                        parts.push(Fragment::Parameter(expansion));
                    }
                    // $'...' — ANSI-C quoting (Bash)
                    Some(&'\'') if !in_double_quote && options.ansi_c_quoting => {
                        chars.next(); // consume '
                        let mut content = String::new();
                        loop {
                            match chars.next() {
                                Some('\'') => break,
                                Some('\\') => {
                                    // Keep escape sequences literally — the executor interprets them
                                    content.push('\\');
                                    if let Some(c) = chars.next() {
                                        content.push(c);
                                    }
                                }
                                Some(c) => content.push(c),
                                None => break, // unterminated (lexer already validated)
                            }
                        }
                        parts.push(Fragment::BashAnsiCQuoted(content));
                    }
                    // $"..." — locale translation (Bash)
                    Some(&'"') if !in_double_quote && options.locale_translation => {
                        chars.next(); // consume "
                        let mut dq_content = String::new();
                        loop {
                            match chars.next() {
                                Some('"') => break,
                                Some('\\') => match chars.peek() {
                                    Some(&c)
                                        if c == '$'
                                            || c == '`'
                                            || c == '"'
                                            || c == '\\'
                                            || c == '\n' =>
                                    {
                                        dq_content.push('\\');
                                        dq_content.push(c);
                                        chars.next();
                                    }
                                    _ => {
                                        dq_content.push('\\');
                                    }
                                },
                                Some(c) => dq_content.push(c),
                                None => break,
                            }
                        }
                        let inner_parts = parse_fragments(&dq_content, true, options);
                        parts.push(Fragment::BashLocaleQuoted(inner_parts));
                    }
                    Some(&c) if c.is_ascii_alphanumeric() || c == '_' || is_special_param(c) => {
                        let expansion = parse_simple_parameter(&mut chars);
                        parts.push(Fragment::Parameter(expansion));
                    }
                    _ => {
                        // Lone $ is literal
                        literal.push('$');
                    }
                }
            }

            // Backtick command substitution
            '`' => {
                flush_literal(&mut literal, &mut parts);
                chars.next(); // consume `
                let mut cmd = String::new();
                loop {
                    match chars.next() {
                        Some('`') => break,
                        Some('\\') => {
                            if let Some(c) = chars.next() {
                                if c == '`' || c == '\\' || c == '$' {
                                    cmd.push(c);
                                } else {
                                    cmd.push('\\');
                                    cmd.push(c);
                                }
                            }
                        }
                        Some(c) => cmd.push(c),
                        None => break,
                    }
                }
                let stmts = parse_command_substitution(&cmd);
                parts.push(Fragment::CommandSubstitution(stmts));
            }

            // Extended globbing: ?(...), *(...), +(...), @(...), !(...) (Bash)
            '?' | '*' | '+' | '@' | '!' if !in_double_quote && options.extglob => {
                // Peek ahead to see if `(` follows
                let mut lookahead = chars.clone();
                lookahead.next(); // skip current char
                if lookahead.peek() == Some(&'(') {
                    flush_literal(&mut literal, &mut parts);
                    let kind = match ch {
                        '?' => ExtGlobKind::ZeroOrOne,
                        '*' => ExtGlobKind::ZeroOrMore,
                        '+' => ExtGlobKind::OneOrMore,
                        '@' => ExtGlobKind::ExactlyOne,
                        '!' => ExtGlobKind::Not,
                        _ => unreachable!(),
                    };
                    chars.next(); // consume prefix char
                    chars.next(); // consume (
                    let mut pattern = String::new();
                    let mut depth = 1;
                    loop {
                        match chars.next() {
                            Some('(') => {
                                depth += 1;
                                pattern.push('(');
                            }
                            Some(')') => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                pattern.push(')');
                            }
                            Some(c) => pattern.push(c),
                            None => break,
                        }
                    }
                    parts.push(Fragment::BashExtGlob { kind, pattern });
                } else {
                    // Not followed by ( — handle as glob or literal
                    match ch {
                        '*' => {
                            flush_literal(&mut literal, &mut parts);
                            chars.next();
                            parts.push(Fragment::Glob(GlobChar::Star));
                        }
                        '?' => {
                            flush_literal(&mut literal, &mut parts);
                            chars.next();
                            parts.push(Fragment::Glob(GlobChar::Question));
                        }
                        _ => {
                            // +, @, ! without ( are just literal characters
                            literal.push(ch);
                            chars.next();
                        }
                    }
                }
            }

            // Glob characters (only outside double quotes)
            '*' if !in_double_quote => {
                flush_literal(&mut literal, &mut parts);
                chars.next();
                parts.push(Fragment::Glob(GlobChar::Star));
            }
            '?' if !in_double_quote => {
                flush_literal(&mut literal, &mut parts);
                chars.next();
                parts.push(Fragment::Glob(GlobChar::Question));
            }
            '[' if !in_double_quote => {
                flush_literal(&mut literal, &mut parts);
                chars.next();
                parts.push(Fragment::Glob(GlobChar::BracketOpen));
                // Read until ] and push as literal (the bracket expression content)
                let mut bracket_content = String::new();
                // Handle negation and first ]
                if chars.peek() == Some(&'!') || chars.peek() == Some(&'^') {
                    bracket_content.push(chars.next().unwrap());
                }
                if chars.peek() == Some(&']') {
                    bracket_content.push(chars.next().unwrap());
                }
                loop {
                    match chars.next() {
                        Some(']') => {
                            bracket_content.push(']');
                            break;
                        }
                        Some(c) => bracket_content.push(c),
                        None => break,
                    }
                }
                if !bracket_content.is_empty() {
                    parts.push(Fragment::Literal(bracket_content));
                }
            }

            // Brace expansion: {a,b,c} or {1..5} (Bash)
            '{' if !in_double_quote && options.brace_expansion => {
                // Scan ahead to find matching } and check for , or ..
                if let Some(brace) = try_parse_brace_expansion(&mut chars) {
                    flush_literal(&mut literal, &mut parts);
                    parts.push(Fragment::BashBraceExpansion(brace));
                } else {
                    literal.push(ch);
                    chars.next();
                }
            }

            // Regular character
            _ => {
                literal.push(ch);
                chars.next();
            }
        }
    }

    flush_literal(&mut literal, &mut parts);
    parts
}

/// Try to parse a brace expansion starting at `{`. Returns `None` if the
/// content between `{` and `}` doesn't look like a brace expansion (no `,` or `..`).
/// Advances `chars` past the `}` on success, leaves it at `{` on failure.
fn try_parse_brace_expansion(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Option<BraceExpansionKind> {
    // Scan ahead to find the matching } and extract content
    let mut lookahead = chars.clone();
    lookahead.next(); // skip {
    let mut content = String::new();
    let mut depth = 1;
    loop {
        match lookahead.next() {
            Some('{') => {
                depth += 1;
                content.push('{');
            }
            Some('}') => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                content.push('}');
            }
            Some('\\') => {
                content.push('\\');
                if let Some(c) = lookahead.next() {
                    content.push(c);
                }
            }
            Some(c) => content.push(c),
            None => return None, // no matching }
        }
    }

    // Check if content looks like a brace expansion
    if content.contains("..") {
        // Sequence: {start..end} or {start..end..step}
        let parts: Vec<&str> = content.splitn(3, "..").collect();
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            // Advance the real iterator past the closing }
            chars.next(); // {
            for _ in 0..content.len() {
                chars.next();
            }
            chars.next(); // }
            return Some(BraceExpansionKind::Sequence {
                start: parts[0].to_string(),
                end: parts[1].to_string(),
                step: parts.get(2).map(|s| s.to_string()),
            });
        }
    }

    if content.contains(',') {
        // List: {a,b,c}
        // Split by top-level commas (not inside nested braces)
        let items: Vec<&str> = content.split(',').collect();
        let item_fragments: Vec<Vec<Fragment>> = items
            .into_iter()
            .map(|s| {
                if s.is_empty() {
                    vec![]
                } else {
                    vec![Fragment::Literal(s.to_string())]
                }
            })
            .collect();

        // Advance the real iterator past the closing }
        chars.next(); // {
        for _ in 0..content.len() {
            chars.next();
        }
        chars.next(); // }
        return Some(BraceExpansionKind::List(item_fragments));
    }

    None // not a brace expansion — no , or ..
}

fn flush_literal(literal: &mut String, parts: &mut Vec<Fragment>) {
    if !literal.is_empty() {
        parts.push(Fragment::Literal(std::mem::take(literal)));
    }
}

#[cfg(test)]
#[path = "word/tests.rs"]
mod tests;
