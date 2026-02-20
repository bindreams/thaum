pub(crate) mod arith_expr;
mod bash;
mod commands;
mod compound;
mod expressions;
mod helpers;
mod test_expr;
mod word_collect;

use crate::ast::*;
use crate::dialect::ParseOptions;
use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use helpers::keyword_display_name;

pub use helpers::expr_span;

/// Parse a complete shell program from source text (POSIX mode).
pub fn parse(input: &str) -> Result<Program, ParseError> {
    parse_with_options(input, ParseOptions::default())
}

/// Parse a complete shell program with the given options.
pub fn parse_with_options(input: &str, options: ParseOptions) -> Result<Program, ParseError> {
    let mut parser = Parser::new(input, options)?;
    parser.parse_program()
}

/// Wrap an expression in a Statement with Sequential mode.
fn stmt(expression: Expression, span: Span) -> Statement {
    Statement {
        expression,
        mode: ExecutionMode::Sequential,
        span,
    }
}

pub(crate) struct Parser {
    pub(super) lexer: Lexer,
    pub(super) options: ParseOptions,
}

impl Parser {
    pub fn new(input: &str, options: ParseOptions) -> Result<Self, ParseError> {
        let lexer = Lexer::from_str(input, options.clone());
        Ok(Parser { lexer, options })
    }

    // ================================================================
    // Helper methods
    //
    // Word collection code (word_collect.rs) deliberately bypasses
    // these to see Whitespace tokens as word boundaries.
    // ================================================================

    /// Consume the current token if it matches the expected operator token.
    fn eat(&mut self, expected: &Token) -> Result<bool, ParseError> {
        self.lexer.eat_whitespace()?;
        if self.lexer.peek()?.token == *expected {
            self.lexer.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Consume the current token if it is a keyword matching the given string.
    fn eat_keyword(&mut self, keyword: &str) -> Result<bool, ParseError> {
        self.lexer.eat_whitespace()?;
        let tok = self.lexer.peek()?.token.clone();
        if tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, keyword) {
            self.lexer.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Expect and consume an operator token. Returns error if not matched.
    fn expect(&mut self, expected: &Token) -> Result<SpannedToken, ParseError> {
        self.lexer.eat_whitespace()?;
        if self.lexer.peek()?.token == *expected {
            self.lexer.advance()
        } else {
            Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: expected.display_name().to_string(),
                span: self.lexer.peek()?.span,
            })
        }
    }

    /// Expect and consume a keyword (a lone Literal matching the string).
    fn expect_keyword(&mut self, keyword: &str) -> Result<SpannedToken, ParseError> {
        self.lexer.eat_whitespace()?;
        let tok = self.lexer.peek()?.token.clone();
        if tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, keyword) {
            self.lexer.advance()
        } else {
            Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: keyword_display_name(keyword),
                span: self.lexer.peek()?.span,
            })
        }
    }

    /// Expect a keyword that closes a construct. Produces UnclosedConstruct
    /// error on EOF for better error messages.
    fn expect_closing_keyword(
        &mut self,
        keyword: &str,
        opening: &str,
        opening_span: Span,
    ) -> Result<SpannedToken, ParseError> {
        self.lexer.eat_whitespace()?;
        let tok = self.lexer.peek()?.token.clone();
        if tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, keyword) {
            return self.lexer.advance();
        }
        if tok == Token::Eof {
            Err(ParseError::UnclosedConstruct {
                keyword: keyword_display_name(keyword),
                opening: opening.to_string(),
                span: opening_span,
            })
        } else {
            Err(ParseError::UnexpectedToken {
                found: tok.display_name().to_string(),
                expected: keyword_display_name(keyword),
                span: self.lexer.peek()?.span,
            })
        }
    }

    fn skip_linebreak(&mut self) -> Result<(), ParseError> {
        self.lexer.eat_whitespace()?;
        while self.lexer.peek()?.token == Token::Newline {
            self.lexer.advance()?;
            self.lexer.eat_whitespace()?;
        }
        Ok(())
    }

    /// Consume heredoc body tokens that belong to the just-parsed statement.
    pub(super) fn consume_heredoc_bodies(&mut self) -> Result<Vec<String>, ParseError> {
        self.lexer.eat_whitespace()?;
        if self.lexer.peek()?.token != Token::Newline {
            return Ok(Vec::new());
        }

        let result = self.lexer.speculate(|s| {
            s.advance()?; // consume Newline tentatively
            s.eat_whitespace()?;
            if !matches!(s.peek()?.token, Token::HereDocBody(_)) {
                return Ok(None);
            }
            let mut bodies = Vec::new();
            while let Token::HereDocBody(body) = &s.peek()?.token {
                bodies.push(body.clone());
                s.advance()?;
            }
            Ok(Some(bodies))
        })?;

        Ok(result.unwrap_or_default())
    }

    fn skip_newline_list(&mut self) -> Result<bool, ParseError> {
        self.lexer.eat_whitespace()?;
        if self.lexer.peek()?.token != Token::Newline {
            return Ok(false);
        }
        while self.lexer.peek()?.token == Token::Newline {
            self.lexer.advance()?;
            self.lexer.eat_whitespace()?;
        }
        Ok(true)
    }
}


#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
