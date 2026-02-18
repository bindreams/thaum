//! Parameter expansion parsing functions.

use crate::ast::*;
use crate::span::Span;

/// Returns `true` if `c` is a POSIX special parameter character.
pub(super) fn is_special_param(c: char) -> bool {
    matches!(c, '@' | '*' | '#' | '?' | '-' | '$' | '!' | '0')
}

/// Parse a simple parameter expansion: `$name` or `$N` or `$@` etc.
pub(super) fn parse_simple_parameter(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> ParameterExpansion {
    let mut name = String::new();

    if let Some(&c) = chars.peek() {
        if is_special_param(c) {
            name.push(c);
            chars.next();
            return ParameterExpansion::Simple(name);
        }
    }

    // Regular name: [A-Za-z_][A-Za-z0-9_]* or digit
    if let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            name.push(c);
            chars.next();
            return ParameterExpansion::Simple(name);
        }
    }

    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphanumeric() || c == '_' {
            name.push(c);
            chars.next();
        } else {
            break;
        }
    }

    ParameterExpansion::Simple(name)
}

/// Parse a brace parameter expansion: `${...}`.
/// Chars iterator should be positioned after the `{`.
pub(super) fn parse_brace_expansion(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> ParameterExpansion {
    // Collect everything up to the matching }
    let mut content = String::new();
    let mut depth = 1;

    while let Some(c) = chars.next() {
        match c {
            '{' => {
                depth += 1;
                content.push(c);
            }
            '}' => {
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
            _ => content.push(c),
        }
    }

    // Now parse the content
    // Check for ${#name} (length)
    if content.starts_with('#')
        && content.len() > 1
        && !content.contains(':')
        && !content.contains('%')
    {
        let name = content[1..].to_string();
        return ParameterExpansion::Complex {
            name,
            operator: Some(ParamOp::Length),
            argument: None,
        };
    }

    // Find the operator
    let name_end = content
        .find([':', '%', '#', '-', '=', '?', '+'])
        .unwrap_or(content.len());

    let name = content[..name_end].to_string();

    if name_end >= content.len() {
        // Simple: ${name}
        return ParameterExpansion::Complex {
            name,
            operator: None,
            argument: None,
        };
    }

    let rest = &content[name_end..];
    let (op, arg_start) = parse_param_operator(rest);

    let argument = if arg_start < rest.len() {
        let arg_str = rest[arg_start..].to_string();
        Some(Box::new(Word {
            parts: vec![Fragment::Literal(arg_str)],
            span: Span::empty(0),
        }))
    } else {
        None
    };

    ParameterExpansion::Complex {
        name,
        operator: op,
        argument,
    }
}

/// Parse parameter operator from the remaining string.
/// Returns (operator, byte offset where argument starts).
pub(super) fn parse_param_operator(s: &str) -> (Option<ParamOp>, usize) {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return (None, 0);
    }

    match bytes[0] {
        b':' if bytes.len() > 1 => match bytes[1] {
            b'-' => (Some(ParamOp::Default), 2),
            b'=' => (Some(ParamOp::DefaultAssign), 2),
            b'?' => (Some(ParamOp::Error), 2),
            b'+' => (Some(ParamOp::Alternative), 2),
            _ => (None, 0),
        },
        b'-' => (Some(ParamOp::Default), 1),
        b'=' => (Some(ParamOp::DefaultAssign), 1),
        b'?' => (Some(ParamOp::Error), 1),
        b'+' => (Some(ParamOp::Alternative), 1),
        b'%' => {
            if bytes.len() > 1 && bytes[1] == b'%' {
                (Some(ParamOp::TrimLargeSuffix), 2)
            } else {
                (Some(ParamOp::TrimSmallSuffix), 1)
            }
        }
        b'#' => {
            if bytes.len() > 1 && bytes[1] == b'#' {
                (Some(ParamOp::TrimLargePrefix), 2)
            } else {
                (Some(ParamOp::TrimSmallPrefix), 1)
            }
        }
        _ => (None, 0),
    }
}
