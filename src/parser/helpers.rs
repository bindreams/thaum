use crate::ast::*;
use crate::dialect::ParseOptions;
use crate::span::Span;
use crate::token::Token;

/// Check if a token is a keyword matching the given string.
pub(super) fn is_keyword(token: &Token, keyword: &str) -> bool {
    matches!(token, Token::Word(w) if w == keyword)
}

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

pub(super) fn make_word(s: String, span: Span, options: &ParseOptions) -> Word {
    crate::word::parse_word(&s, span, options)
}

pub(super) fn make_argument(s: String, span: Span, options: &ParseOptions) -> Argument {
    crate::word::parse_argument(&s, span, options)
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
