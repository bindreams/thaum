//! Recursive-descent parser. Promotes `Literal` tokens to keywords based on
//! grammatical context (the lexer is context-free). Holds the `Lexer` directly
//! with no intermediate token-stream layer. A post-parse `HereDocFiller` fold
//! patches heredoc bodies from the lexer's side queue into the AST.

pub(crate) mod arith_expr;
mod bash;
mod commands;
mod compound;
mod expressions;
mod helpers;
mod test_expr;
mod word_collect;

use crate::ast::*;
use crate::dialect::ShellOptions;
use crate::error::ParseError;
use crate::fold::Fold;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use helpers::keyword_display_name;

pub use helpers::expr_span;

/// Parse a complete shell program from source text (POSIX mode).
pub fn parse(input: &str) -> Result<Program, ParseError> {
    parse_with_options(input, ShellOptions::default())
}

/// Parse a complete shell program with the given options.
pub fn parse_with_options(input: &str, options: ShellOptions) -> Result<Program, ParseError> {
    let mut parser = Parser::new(input, options)?;
    let program = parser.parse_program()?;
    // Post-parse: fill heredoc bodies that were side-queued during lexing.
    let program = HereDocFiller(&mut parser.lexer).fold_program(program);
    Ok(program)
}

/// Fills empty heredoc redirect bodies from the lexer's completed-bodies queue.
struct HereDocFiller<'a>(&'a mut Lexer);

impl crate::fold::Fold for HereDocFiller<'_> {
    fn fold_redirect(&mut self, mut redirect: Redirect) -> Redirect {
        if let RedirectKind::HereDoc { ref mut body, .. } = redirect.kind {
            if body.is_empty() {
                if let Some(b) = self.0.take_heredoc_body() {
                    *body = b;
                }
            }
        }
        crate::fold::fold_redirect(self, redirect)
    }
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
    pub(super) options: ShellOptions,
    /// Set inside `( ... )` groups in `[[ ]]`. Tells `consume_regex_pattern`
    /// that an unmatched `)` closes the group rather than being regex content.
    pub(super) in_test_group: bool,
}

impl Parser {
    pub fn new(input: &str, options: ShellOptions) -> Result<Self, ParseError> {
        let lexer = Lexer::from_str(input, options.clone());
        Ok(Parser {
            lexer,
            options,
            in_test_group: false,
        })
    }

    // Helper methods --------------------------------------------------------------------------------------------------
    //
    // Word collection code (word_collect.rs) deliberately bypasses
    // these to see Whitespace tokens as word boundaries.

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
            // No eat_whitespace here: after Newline, LastScanned::Other → WS suppressed
        }
        Ok(())
    }

    fn skip_newline_list(&mut self) -> Result<bool, ParseError> {
        self.lexer.eat_whitespace()?;
        if self.lexer.peek()?.token != Token::Newline {
            return Ok(false);
        }
        while self.lexer.peek()?.token == Token::Newline {
            self.lexer.advance()?;
            // No eat_whitespace: after Newline, LastScanned::Other → WS suppressed
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
