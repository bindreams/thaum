use crate::span::Span;
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum LexError {
    #[error("unexpected character '{ch}'")]
    UnexpectedChar { ch: char, span: Span },

    #[error("unterminated single quote")]
    UnterminatedSingleQuote { span: Span },

    #[error("unterminated double quote")]
    UnterminatedDoubleQuote { span: Span },

    #[error("unterminated here-document (delimiter: '{delimiter}')")]
    UnterminatedHereDoc { delimiter: String, span: Span },

    #[error("unterminated backquote")]
    UnterminatedBackquote { span: Span },

    #[error("unterminated {kind}")]
    UnterminatedExpansion { kind: String, span: Span },
}

impl LexError {
    pub fn span(&self) -> Option<Span> {
        match self {
            LexError::UnexpectedChar { span, .. }
            | LexError::UnterminatedSingleQuote { span }
            | LexError::UnterminatedDoubleQuote { span }
            | LexError::UnterminatedHereDoc { span, .. }
            | LexError::UnterminatedBackquote { span }
            | LexError::UnterminatedExpansion { span, .. } => Some(*span),
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("{0}")]
    Lex(#[from] LexError),

    #[error("unexpected token {found}, expected {expected}")]
    UnexpectedToken {
        found: String,
        expected: String,
        span: Span,
    },

    #[error("unexpected end of input, expected {expected}")]
    UnexpectedEof { expected: String },

    #[error("missing '{keyword}' to close '{opening}'")]
    UnclosedConstruct {
        keyword: String,
        opening: String,
        span: Span,
    },
}

impl ParseError {
    /// Extract the source span associated with this error, if any.
    pub fn span(&self) -> Option<Span> {
        match self {
            ParseError::Lex(e) => e.span(),
            ParseError::UnexpectedToken { span, .. } => Some(*span),
            ParseError::UnexpectedEof { .. } => None,
            ParseError::UnclosedConstruct { span, .. } => Some(*span),
        }
    }
}
