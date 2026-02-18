//! Command substitution and arithmetic expansion helpers.

use crate::ast::Statement;

/// Parse a command substitution body into statements.
pub(super) fn parse_command_substitution(cmd: &str) -> Vec<Statement> {
    crate::parser::parse(cmd)
        .map(|prog| prog.statements)
        .unwrap_or_default()
}

/// Read content inside `$(...)`, handling nested parentheses.
pub(super) fn read_until_matching_paren(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> String {
    let mut content = String::new();
    let mut depth = 1;

    while let Some(c) = chars.next() {
        match c {
            '(' => {
                depth += 1;
                content.push(c);
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                content.push(c);
            }
            '\\' => {
                content.push(c);
                if let Some(next) = chars.next() {
                    content.push(next);
                }
            }
            '\'' => {
                content.push(c);
                // Read until closing quote
                loop {
                    match chars.next() {
                        Some('\'') => {
                            content.push('\'');
                            break;
                        }
                        Some(ch) => content.push(ch),
                        None => break,
                    }
                }
            }
            '"' => {
                content.push(c);
                loop {
                    match chars.next() {
                        Some('"') => {
                            content.push('"');
                            break;
                        }
                        Some('\\') => {
                            content.push('\\');
                            if let Some(ch) = chars.next() {
                                content.push(ch);
                            }
                        }
                        Some(ch) => content.push(ch),
                        None => break,
                    }
                }
            }
            _ => content.push(c),
        }
    }

    content
}

/// Read arithmetic expansion content `$((...))`, consuming until `))`.
pub(super) fn read_balanced_parens(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut content = String::new();
    let mut depth = 2; // We already consumed $((

    while let Some(c) = chars.next() {
        match c {
            '(' => {
                depth += 1;
                content.push(c);
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                if depth == 1 && chars.peek() == Some(&')') {
                    // This closes the $((
                    chars.next();
                    break;
                }
                content.push(c);
            }
            _ => content.push(c),
        }
    }

    content
}
