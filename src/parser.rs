pub(crate) mod arith_expr;
mod bash;
mod commands;
mod compound;
mod expressions;
mod helpers;
mod test_expr;
mod token_stream;

use crate::ast::*;
use crate::dialect::ParseOptions;
use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use helpers::{is_keyword, keyword_display_name};
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
    // ================================================================

    /// Consume the current token if it matches the expected operator token.
    fn eat(&mut self, expected: &Token) -> Result<bool, ParseError> {
        if self.stream.peek()?.token == *expected {
            self.stream.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Consume the current token if it is a keyword matching the given string.
    fn eat_keyword(&mut self, keyword: &str) -> Result<bool, ParseError> {
        if is_keyword(&self.stream.peek()?.token, keyword) {
            self.stream.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Expect and consume an operator token. Returns error if not matched.
    fn expect(&mut self, expected: &Token) -> Result<SpannedToken, ParseError> {
        let peeked = self.stream.peek()?;
        if peeked.token == *expected {
            self.stream.advance()
        } else {
            Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: expected.display_name().to_string(),
                span: self.stream.peek()?.span,
            })
        }
    }

    /// Expect and consume a keyword (reserved word that comes as Word("...")).
    fn expect_keyword(&mut self, keyword: &str) -> Result<SpannedToken, ParseError> {
        if is_keyword(&self.stream.peek()?.token, keyword) {
            self.stream.advance()
        } else {
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
        if is_keyword(&self.stream.peek()?.token, keyword) {
            return self.stream.advance();
        }
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
        while self.stream.peek()?.token == Token::Newline {
            self.stream.advance()?;
        }
        Ok(())
    }

    /// Consume heredoc body tokens that belong to the just-parsed statement.
    ///
    /// After a command with heredoc redirects, the token stream contains:
    /// `Newline, HereDocBody, HereDocBody, ...` The Newline is consumed as
    /// part of statement termination, and the HereDocBody tokens are collected
    /// and returned so the caller can fill them into the expression's redirects.
    pub(super) fn consume_heredoc_bodies(&mut self) -> Result<Vec<String>, ParseError> {
        // Check if there's a Newline followed by HereDocBody
        if self.stream.peek()?.token != Token::Newline {
            return Ok(Vec::new());
        }

        // Peek past the Newline to see if HereDocBody follows
        // We need to use checkpoint/rewind to avoid consuming the Newline
        // if there are no HereDocBody tokens after it.
        let cp = self.stream.checkpoint();
        self.stream.advance()?; // consume Newline tentatively

        if !matches!(self.stream.peek()?.token, Token::HereDocBody(_)) {
            // No heredoc bodies — put the Newline back
            self.stream.rewind(cp);
            return Ok(Vec::new());
        }
        self.stream.release(cp);

        // Collect all HereDocBody tokens
        let mut bodies = Vec::new();
        while let Token::HereDocBody(body) = &self.stream.peek()?.token {
            bodies.push(body.clone());
            self.stream.advance()?;
        }

        Ok(bodies)
    }

    fn skip_newline_list(&mut self) -> Result<bool, ParseError> {
        if self.stream.peek()?.token != Token::Newline {
            return Ok(false);
        }
        while self.stream.peek()?.token == Token::Newline {
            self.stream.advance()?;
        }
        Ok(true)
    }

    /// Returns true if the current token is any Word (including reserved words).
    /// Use this in argument position where reserved words are just words.
    fn is_word(&mut self) -> Result<bool, ParseError> {
        Ok(matches!(self.stream.peek()?.token, Token::Word(_)))
    }

    /// Returns true if a word string is a "closing" reserved keyword that
    /// cannot start a new command. These are structure keywords that
    /// terminate or separate compound command clauses.
    fn is_closing_keyword(w: &str) -> bool {
        matches!(
            w,
            "then" | "else" | "elif" | "fi" | "do" | "done" | "esac" | "}" | "in"
        )
    }

    fn can_start_command(&mut self) -> Result<bool, ParseError> {
        let tok = &self.stream.peek()?.token;
        Ok(match tok {
            Token::Word(w) => {
                // Words can start a command UNLESS they are closing keywords
                // (then, fi, done, do, else, elif, esac, }, in)
                !Self::is_closing_keyword(w)
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
            _ => false,
        })
    }

    fn is_redirect_op(&mut self) -> Result<bool, ParseError> {
        let tok = &self.stream.peek()?.token;
        Ok(tok.is_redirect_op() || matches!(tok, Token::IoNumber(_)))
    }

    /// Check if a Word token is a compound command keyword.
    fn is_compound_keyword(w: &str) -> bool {
        matches!(w, "if" | "while" | "until" | "for" | "case" | "{")
    }

    /// Check if a Word token is a keyword that starts a compound command,
    /// including Bash extensions.
    fn is_compound_start_word(&self, w: &str) -> bool {
        Self::is_compound_keyword(w) || (w == "select" && self.options.select)
    }

    /// Check if the current token starts a compound command.
    fn is_compound_start(&mut self) -> Result<bool, ParseError> {
        let tok = self.stream.peek()?.token.clone();
        Ok(match &tok {
            Token::Word(w) => self.is_compound_start_word(w),
            Token::LParen | Token::BashDblLBracket => true,
            _ => false,
        })
    }
}


#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
