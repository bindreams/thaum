//! Parameter expansion parsing.

use crate::ast::*;
use crate::span::Span;

/// Parse the raw content of a brace parameter expansion (content between `${` and `}`).
///
/// Handles `${name}`, `${name:-default}`, `${#name}`, `${name%%pattern}`, etc.
/// When `case_modification` is true, also recognizes `^`, `^^`, `,`, `,,` as
/// operators (Bash 4.0+ `${var^}`, `${var^^}`, `${var,}`, `${var,,}`).
/// When `parameter_transform` is true, also recognizes `@X` as operators
/// (Bash 4.4+ `${var@Q}`, `${var@a}`, etc.). The `parameter_transform_51`
/// flag additionally enables `@L`/`@U`/`@u`/`@K`/`@k` (Bash 5.1+).
pub(crate) fn parse_brace_param_content(
    content: &str,
    case_modification: bool,
    parameter_transform: bool,
    parameter_transform_51: bool,
) -> ParameterExpansion {
    // Detect indirect expansion prefix `!`
    let (indirect, content) = if content.starts_with('!') && content.len() > 1 {
        (true, &content[1..])
    } else {
        (false, content)
    };

    // Check for ${#name} (length) — only when not indirect
    if !indirect && content.starts_with('#') && content.len() > 1 && !content.contains(':') && !content.contains('%') {
        let name = content[1..].to_string();
        return ParameterExpansion::Complex {
            name,
            indirect: false,
            operator: Some(ParamOp::Length),
            argument: None,
        };
    }

    // Check for @X transformation operator at the end of the name.
    // The `@` operator appears after the variable name: `${name@Q}`.
    // We must NOT confuse `@` in array subscripts (`${arr[@]}`) with
    // a transform operator.
    if parameter_transform {
        if let Some(at_pos) = content.rfind('@') {
            let after_at = &content[at_pos + 1..];
            if after_at.len() == 1 {
                let op = match after_at.as_bytes()[0] {
                    b'Q' => Some(ParamOp::TransformQuote),
                    b'E' => Some(ParamOp::TransformEscape),
                    b'P' => Some(ParamOp::TransformPrompt),
                    b'A' => Some(ParamOp::TransformAssignment),
                    b'a' => Some(ParamOp::TransformAttributes),
                    b'L' if parameter_transform_51 => Some(ParamOp::TransformLower),
                    b'U' if parameter_transform_51 => Some(ParamOp::TransformUpper),
                    b'u' if parameter_transform_51 => Some(ParamOp::TransformCapitalize),
                    b'K' if parameter_transform_51 => Some(ParamOp::TransformKeyValue),
                    b'k' if parameter_transform_51 => Some(ParamOp::TransformKeys),
                    _ => None,
                };
                if let Some(op) = op {
                    let name = content[..at_pos].to_string();
                    return ParameterExpansion::Complex {
                        name,
                        indirect,
                        operator: Some(op),
                        argument: None,
                    };
                }
            }
        }
    }

    // Find the operator — the set of characters that terminate the name.
    let name_end = if case_modification {
        content
            .find([':', '%', '#', '-', '=', '?', '+', '^', ','])
            .unwrap_or(content.len())
    } else {
        content
            .find([':', '%', '#', '-', '=', '?', '+'])
            .unwrap_or(content.len())
    };

    let name = content[..name_end].to_string();

    if name_end >= content.len() {
        return ParameterExpansion::Complex {
            name,
            indirect,
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
        indirect,
        operator: op,
        argument,
    }
}

/// Parse parameter operator from the remaining string.
/// Returns (operator, byte offset where argument starts).
fn parse_param_operator(s: &str) -> (Option<ParamOp>, usize) {
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
        b'^' => {
            if bytes.len() > 1 && bytes[1] == b'^' {
                (Some(ParamOp::UpperAll), 2)
            } else {
                (Some(ParamOp::UpperFirst), 1)
            }
        }
        b',' => {
            if bytes.len() > 1 && bytes[1] == b',' {
                (Some(ParamOp::LowerAll), 2)
            } else {
                (Some(ParamOp::LowerFirst), 1)
            }
        }
        _ => (None, 0),
    }
}
