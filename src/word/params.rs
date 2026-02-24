//! Parameter expansion parsing.

use crate::ast::*;
use crate::span::Span;

/// Parse the raw content of a brace parameter expansion (content between `${` and `}`).
///
/// Handles `${name}`, `${name:-default}`, `${#name}`, `${name%%pattern}`, etc.
/// When `case_modification` is true, also recognizes `^`, `^^`, `,`, `,,` as
/// operators (Bash 4.0+ `${var^}`, `${var^^}`, `${var,}`, `${var,,}`).
pub(crate) fn parse_brace_param_content(content: &str, case_modification: bool) -> ParameterExpansion {
    // Check for ${#name} (length)
    if content.starts_with('#') && content.len() > 1 && !content.contains(':') && !content.contains('%') {
        let name = content[1..].to_string();
        return ParameterExpansion::Complex {
            name,
            operator: Some(ParamOp::Length),
            argument: None,
        };
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
