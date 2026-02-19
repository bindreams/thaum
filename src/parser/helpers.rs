use crate::ast::*;
use crate::span::Span;
use crate::token::{GlobKind, Token};

/// Display name for a keyword string (for error messages).
pub(super) fn keyword_display_name(keyword: &str) -> String {
    format!("'{}'", keyword)
}

/// Get the span of a compound command.
pub(super) fn compound_command_span(cmd: &CompoundCommand) -> Span {
    match cmd {
        CompoundCommand::BraceGroup { span, .. }
        | CompoundCommand::Subshell { span, .. }
        | CompoundCommand::ForClause { span, .. }
        | CompoundCommand::CaseClause { span, .. }
        | CompoundCommand::IfClause { span, .. }
        | CompoundCommand::WhileClause { span, .. }
        | CompoundCommand::UntilClause { span, .. }
        | CompoundCommand::BashDoubleBracket { span, .. }
        | CompoundCommand::BashArithmeticCommand { span, .. }
        | CompoundCommand::BashSelectClause { span, .. }
        | CompoundCommand::BashCoproc { span, .. }
        | CompoundCommand::BashArithmeticFor { span, .. } => *span,
    }
}

pub(super) fn argument_span(arg: &Argument) -> Span {
    match arg {
        Argument::Word(w) => w.span,
        Argument::Atom(a) => match a {
            Atom::BashProcessSubstitution { span, .. } => *span,
        },
    }
}

pub(super) fn is_valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Get the span of an expression.
pub fn expr_span(expr: &Expression) -> Span {
    match expr {
        Expression::Command(c) => c.span,
        Expression::Compound { body, redirects } => {
            if let Some(r) = redirects.last() {
                compound_command_span(body).merge(r.span)
            } else {
                compound_command_span(body)
            }
        }
        Expression::FunctionDef(f) => f.span,
        Expression::And { left, right } => expr_span(left).merge(expr_span(right)),
        Expression::Or { left, right } => expr_span(left).merge(expr_span(right)),
        Expression::Pipe { left, right, .. } => expr_span(left).merge(expr_span(right)),
        Expression::Not(inner) => expr_span(inner),
    }
}

/// De-escape a raw literal string: remove backslash escaping.
/// `\\c` -> `c`, other characters pass through unchanged.
pub(super) fn de_escape_literal(s: &str) -> String {
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

/// Convert a fragment token to its source text for raw-text contexts
/// like arithmetic parsing. Does NOT include `$` prefixes.
pub(super) fn fragment_token_to_text(token: &Token) -> &str {
    match token {
        Token::Literal(s) => s,
        Token::SimpleParam(s) => s,
        Token::BraceParam(s) => s,
        Token::Glob(GlobKind::Star) => "*",
        Token::Glob(GlobKind::Question) => "?",
        Token::Glob(GlobKind::BracketOpen) => "[",
        Token::TildePrefix(_) => "~",
        _ => token.display_name().trim_matches('\''),
    }
}

/// Convert a fragment token to its full source text including `$` prefixes.
pub(super) fn fragment_token_to_source(token: &Token) -> String {
    match token {
        Token::Literal(s) => s.clone(),
        Token::SimpleParam(s) => format!("${}", s),
        Token::BraceParam(s) => format!("${{{}}}", s),
        Token::CommandSub(s) => format!("$({})", s),
        Token::BacktickSub(s) => format!("`{}`", s),
        Token::ArithSub(s) => format!("$(({}))", s),
        Token::SingleQuoted(s) => format!("'{}'", s),
        Token::DoubleQuoted(s) => format!("\"{}\"", s),
        Token::Glob(GlobKind::Star) => "*".to_string(),
        Token::Glob(GlobKind::Question) => "?".to_string(),
        Token::Glob(GlobKind::BracketOpen) => "[".to_string(),
        Token::TildePrefix(s) => format!("~{}", s),
        _ => token.display_name().trim_matches('\'').to_string(),
    }
}
