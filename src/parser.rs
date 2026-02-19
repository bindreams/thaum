pub(crate) mod arith_expr;
mod bash;
mod commands;
mod compound;
mod expressions;
mod helpers;
mod test_expr;
mod token_stream;
mod word_collect;

use crate::ast::*;
use crate::dialect::ParseOptions;
use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use helpers::keyword_display_name;
use token_stream::TokenStream;

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

pub(crate) struct Parser<'src> {
    pub(super) stream: TokenStream<'src>,
    pub(super) options: ParseOptions,
}

impl<'src> Parser<'src> {
    pub fn new(input: &'src str, options: ParseOptions) -> Result<Self, ParseError> {
        let lexer = Lexer::new(input, options.clone());
        let stream = TokenStream::new(lexer)?;
        Ok(Parser { stream, options })
    }

    // ================================================================
    // Helper methods
    //
    // These all call skip_blanks() so callers don't have to.
    // Word collection code (word_collect.rs) deliberately bypasses
    // these to see Blank tokens as word boundaries.
    // ================================================================

    /// Consume the current token if it matches the expected operator token.
    fn eat(&mut self, expected: &Token) -> Result<bool, ParseError> {
        self.stream.skip_blanks()?;
        if self.stream.peek()?.token == *expected {
            self.stream.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Consume the current token if it is a keyword matching the given string.
    fn eat_keyword(&mut self, keyword: &str) -> Result<bool, ParseError> {
        if self.is_lone_literal(keyword)? {
            self.stream.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Expect and consume an operator token. Returns error if not matched.
    fn expect(&mut self, expected: &Token) -> Result<SpannedToken, ParseError> {
        self.stream.skip_blanks()?;
        if self.stream.peek()?.token == *expected {
            self.stream.advance()
        } else {
            Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: expected.display_name().to_string(),
                span: self.stream.peek()?.span,
            })
        }
    }

    /// Expect and consume a keyword (a lone Literal matching the string).
    fn expect_keyword(&mut self, keyword: &str) -> Result<SpannedToken, ParseError> {
        if self.is_lone_literal(keyword)? {
            self.stream.advance()
        } else {
            self.stream.skip_blanks()?;
            Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: keyword_display_name(keyword),
                span: self.stream.peek()?.span,
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
        if self.is_lone_literal(keyword)? {
            return self.stream.advance();
        }
        self.stream.skip_blanks()?;
        if self.stream.peek()?.token == Token::Eof {
            Err(ParseError::UnclosedConstruct {
                keyword: keyword_display_name(keyword),
                opening: opening.to_string(),
                span: opening_span,
            })
        } else {
            Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: keyword_display_name(keyword),
                span: self.stream.peek()?.span,
            })
        }
    }

    fn skip_linebreak(&mut self) -> Result<(), ParseError> {
        self.stream.skip_blanks()?;
        while self.stream.peek()?.token == Token::Newline {
            self.stream.advance()?;
            self.stream.skip_blanks()?;
        }
        Ok(())
    }

    /// Consume heredoc body tokens that belong to the just-parsed statement.
    pub(super) fn consume_heredoc_bodies(&mut self) -> Result<Vec<String>, ParseError> {
        self.stream.skip_blanks()?;
        if self.stream.peek()?.token != Token::Newline {
            return Ok(Vec::new());
        }

        let cp = self.stream.checkpoint();
        self.stream.advance()?; // consume Newline tentatively
        self.stream.skip_blanks()?;

        if !matches!(self.stream.peek()?.token, Token::HereDocBody(_)) {
            self.stream.rewind(cp);
            return Ok(Vec::new());
        }
        self.stream.release(cp);

        let mut bodies = Vec::new();
        while let Token::HereDocBody(body) = &self.stream.peek()?.token {
            bodies.push(body.clone());
            self.stream.advance()?;
        }

        Ok(bodies)
    }

    fn skip_newline_list(&mut self) -> Result<bool, ParseError> {
        self.stream.skip_blanks()?;
        if self.stream.peek()?.token != Token::Newline {
            return Ok(false);
        }
        while self.stream.peek()?.token == Token::Newline {
            self.stream.advance()?;
            self.stream.skip_blanks()?;
        }
        Ok(true)
    }

    /// Returns true if the current token starts a word (any fragment token).
    fn is_word(&mut self) -> Result<bool, ParseError> {
        self.stream.skip_blanks()?;
        Ok(self.stream.peek()?.token.is_fragment())
    }

    /// Returns true if a word string is a "closing" reserved keyword that
    /// cannot start a new command.
    fn is_closing_keyword(w: &str) -> bool {
        matches!(
            w,
            "then" | "else" | "elif" | "fi" | "do" | "done" | "esac" | "}" | "in"
        )
    }

    fn can_start_command(&mut self) -> Result<bool, ParseError> {
        self.stream.skip_blanks()?;
        let tok = &self.stream.peek()?.token;
        Ok(match tok {
            Token::Literal(w) => {
                if Self::is_closing_keyword(w) {
                    let next = self.stream.peek_at_offset(1)?;
                    next.token.is_fragment()
                } else {
                    true
                }
            }
            Token::IoNumber(_)
            | Token::LParen
            | Token::RedirectFromFile
            | Token::RedirectToFile
            | Token::HereDocOp
            | Token::HereDocStripOp
            | Token::Append
            | Token::RedirectFromFd
            | Token::RedirectToFd
            | Token::ReadWrite
            | Token::Clobber
            | Token::BashHereStringOp
            | Token::BashRedirectAllOp
            | Token::BashAppendAllOp
            | Token::BashDblLBracket => true,
            _ if tok.is_fragment() => true,
            _ => false,
        })
    }

    fn is_redirect_op(&mut self) -> Result<bool, ParseError> {
        self.stream.skip_blanks()?;
        let tok = &self.stream.peek()?.token;
        Ok(tok.is_redirect_op() || matches!(tok, Token::IoNumber(_)))
    }

    fn is_compound_keyword(w: &str) -> bool {
        matches!(w, "if" | "while" | "until" | "for" | "case" | "{")
    }

    fn is_compound_start_word(&self, w: &str) -> bool {
        Self::is_compound_keyword(w) || (w == "select" && self.options.select)
    }

    /// Check if the current token starts a compound command.
    fn is_compound_start(&mut self) -> Result<bool, ParseError> {
        self.stream.skip_blanks()?;
        let tok = self.stream.peek()?.token.clone();
        Ok(match &tok {
            Token::Literal(w) => {
                if self.is_compound_start_word(w) {
                    let next = self.stream.peek_at_offset(1)?;
                    !next.token.is_fragment()
                } else {
                    false
                }
            }
            Token::LParen | Token::BashDblLBracket => true,
            _ => false,
        })
    }
}


#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
