//! Parameter expansion parsing.

use crate::ast::*;
use crate::dialect::ShellOptions;
use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::Token;

/// Parse the raw content of a brace parameter expansion (content between `${` and `}`).
///
/// Handles `${name}`, `${name:-default}`, `${#name}`, `${name%%pattern}`, etc.
/// Reads `case_modification`, `parameter_transform`, and `parameter_transform_51`
/// from `options` to control which operator forms are recognized.
///
/// The brace-argument (e.g. the `default` in `${name:-default}` or the
/// `pattern` in `${name#pattern}`) is parsed structurally — it may contain
/// `$(..)`, `${..}`, `$((..))`, backticks, and `\` escapes.
///
/// TODO: error spans from inner brace-arg parses are relative to the inner
/// string slice, not the outer source. Same caveat as `parse_command_substitution`.
/// TODO: tilde expansion (`${var:-~user}`) is not recognized in brace-arg
/// context — bash expands `~` here, but DQ-mode does not. Separate feature gap.
pub(crate) fn parse_brace_param_content(
    content: &str,
    options: &ShellOptions,
) -> Result<ParameterExpansion, ParseError> {
    // Detect indirect expansion prefix `!`
    let (indirect, content) = if content.starts_with('!') && content.len() > 1 {
        (true, &content[1..])
    } else {
        (false, content)
    };

    // Check for ${#name} (length) — only when not indirect
    if !indirect && content.starts_with('#') && content.len() > 1 && !content.contains(':') && !content.contains('%') {
        let name = content[1..].to_string();
        return Ok(ParameterExpansion::Complex {
            name,
            indirect: false,
            operator: Some(ParamOp::Length),
            argument: None,
        });
    }

    // Check for @X transformation operator at the end of the name.
    // The `@` operator appears after the variable name: `${name@Q}`.
    // We must NOT confuse `@` in array subscripts (`${arr[@]}`) with
    // a transform operator.
    if options.parameter_transform {
        if let Some(at_pos) = content.rfind('@') {
            let after_at = &content[at_pos + 1..];
            if after_at.len() == 1 {
                let op = match after_at.as_bytes()[0] {
                    b'Q' => Some(ParamOp::TransformQuote),
                    b'E' => Some(ParamOp::TransformEscape),
                    b'P' => Some(ParamOp::TransformPrompt),
                    b'A' => Some(ParamOp::TransformAssignment),
                    b'a' => Some(ParamOp::TransformAttributes),
                    b'L' if options.parameter_transform_51 => Some(ParamOp::TransformLower),
                    b'U' if options.parameter_transform_51 => Some(ParamOp::TransformUpper),
                    b'u' if options.parameter_transform_51 => Some(ParamOp::TransformCapitalize),
                    b'K' if options.parameter_transform_51 => Some(ParamOp::TransformKeyValue),
                    b'k' if options.parameter_transform_51 => Some(ParamOp::TransformKeys),
                    _ => None,
                };
                if let Some(op) = op {
                    let name = content[..at_pos].to_string();
                    return Ok(ParameterExpansion::Complex {
                        name,
                        indirect,
                        operator: Some(op),
                        argument: None,
                    });
                }
            }
        }
    }

    // Find the operator — the set of characters that terminate the name.
    let name_end = if options.case_modification {
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
        return Ok(ParameterExpansion::Complex {
            name,
            indirect,
            operator: None,
            argument: None,
        });
    }

    let rest = &content[name_end..];
    let (op, arg_start) = parse_param_operator(rest);

    let argument = if arg_start < rest.len() {
        let arg_str = &rest[arg_start..];
        let parts = parse_brace_arg(arg_str, options)?;
        Some(Box::new(Word {
            parts,
            // TODO: thread the outer source span through so brace-arg errors
            // can be located in the outer file. Same limitation as the
            // pre-fix flat-literal placeholder.
            span: Span::empty(0),
        }))
    } else {
        None
    };

    Ok(ParameterExpansion::Complex {
        name,
        indirect,
        operator: op,
        argument,
    })
}

/// Parse the argument of a parameter expansion (the part after `:-`, `#`,
/// `%`, etc.) into a list of fragments. Spawns an inner Lexer in DoubleQuote
/// mode to recognize `$(..)`, `${..}`, `$((..))`, backtick, and `\` escapes.
///
/// Tilde expansion and word splitting are NOT recognized inside brace-arg
/// context, matching DoubleQuote mode semantics. Tilde-in-brace-arg is a
/// known feature gap (see `parse_brace_param_content`'s TODO).
fn parse_brace_arg(raw: &str, options: &ShellOptions) -> Result<Vec<Fragment>, ParseError> {
    let mut inner_lexer = Lexer::new_double_quote_mode(raw, options.clone());
    let mut fragments = Vec::new();
    loop {
        let tok = inner_lexer.next_token()?;
        if tok.token == Token::Eof {
            break;
        }
        let frag = crate::word::token_to_fragment(tok, options)?;
        fragments.push(frag);
    }
    Ok(crate::word::merge_adjacent_literals(fragments))
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
