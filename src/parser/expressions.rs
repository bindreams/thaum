//! Top-level expression parsing: programs, lists, pipelines, `&&`/`||`,
//! and `!` negation. Implements the operator-precedence layers above
//! individual commands.

use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::*;
use super::Parser;

impl Parser {
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        // No eat_whitespace: start of input → LastScanned::Other → WS suppressed
        let start_span = self.lexer.peek()?.span;
        self.skip_linebreak()?;

        let mut lines = Vec::new();
        while self.lexer.peek()?.token != Token::Eof {
            let mut line = Vec::new();
            self.parse_list_into(&mut line)?;
            lines.push(line);
            self.skip_linebreak()?;
        }

        let end_span = self.lexer.peek()?.span;
        Ok(Program {
            span: if lines.is_empty() {
                start_span
            } else {
                start_span.merge(end_span)
            },
            lines,
        })
    }

    pub(super) fn parse_list_into(&mut self, out: &mut Vec<Statement>) -> Result<(), ParseError> {
        let mut expr = self.parse_and_or()?;
        let mut span = expr_span(&expr);

        loop {
            self.lexer.eat_whitespace()?;
            match self.lexer.peek()?.token {
                Token::Semicolon => {
                    out.push(Statement {
                        expression: expr,
                        mode: ExecutionMode::Terminated,
                        span,
                    });
                    self.lexer.advance()?;
                    // If a newline follows `;`, this line ends here.
                    // The newline will be consumed by the caller's skip_linebreak.
                    self.lexer.eat_whitespace()?;
                    if self.lexer.peek()?.token == Token::Newline
                        || self.lexer.peek()?.token == Token::Eof
                    {
                        return Ok(());
                    }
                    let tok = self.lexer.peek()?.token.clone();
                    if tok.can_start_command(&self.lexer.peek_at_offset(1)?.token) {
                        expr = self.parse_and_or()?;
                        span = expr_span(&expr);
                    } else {
                        return Ok(());
                    }
                }
                Token::Ampersand => {
                    out.push(Statement {
                        expression: expr,
                        mode: ExecutionMode::Background,
                        span,
                    });
                    self.lexer.advance()?;
                    // If a newline follows `&`, this line ends here.
                    self.lexer.eat_whitespace()?;
                    if self.lexer.peek()?.token == Token::Newline
                        || self.lexer.peek()?.token == Token::Eof
                    {
                        return Ok(());
                    }
                    let tok = self.lexer.peek()?.token.clone();
                    if tok.can_start_command(&self.lexer.peek_at_offset(1)?.token) {
                        expr = self.parse_and_or()?;
                        span = expr_span(&expr);
                    } else {
                        return Ok(());
                    }
                }
                Token::Newline => break,
                _ => break,
            }
        }

        out.push(super::stmt(expr, span));
        Ok(())
    }

    pub(super) fn parse_and_or(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_pipeline()?;

        loop {
            self.lexer.eat_whitespace()?;
            match self.lexer.peek()?.token {
                Token::AndIf => {
                    self.lexer.advance()?;
                    self.skip_linebreak()?;
                    let right = self.parse_pipeline()?;
                    left = Expression::And {
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                Token::OrIf => {
                    self.lexer.advance()?;
                    self.skip_linebreak()?;
                    let right = self.parse_pipeline()?;
                    left = Expression::Or {
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_pipeline(&mut self) -> Result<Expression, ParseError> {
        let negated = self.eat_keyword("!")?;

        let mut left = self.parse_leaf_expression()?;

        loop {
            self.lexer.eat_whitespace()?;
            let pipe_token = &self.lexer.peek()?.token;
            let stderr = match pipe_token {
                Token::Pipe => false,
                Token::BashPipeAmpersand => true,
                _ => break,
            };
            self.lexer.advance()?;
            self.skip_linebreak()?;
            let right = self.parse_leaf_expression()?;
            left = Expression::Pipe {
                left: Box::new(left),
                right: Box::new(right),
                stderr,
            };
        }

        if negated {
            left = Expression::Not(Box::new(left));
        }

        Ok(left)
    }

    fn parse_leaf_expression(&mut self) -> Result<Expression, ParseError> {
        self.lexer.eat_whitespace()?;
        let tok = self.lexer.peek()?.token.clone();
        match &tok {
            Token::Literal(w) => {
                let is_lone = {
                    let next = self.lexer.peek_at_offset(1)?;
                    !next.token.is_fragment()
                };

                if is_lone {
                    match w.as_str() {
                        "if" | "while" | "until" | "for" | "case" | "{" => {
                            return self.parse_compound_expression();
                        }
                        "select" if self.options.select => {
                            return self.parse_compound_expression();
                        }
                        "coproc" if self.options.coproc => {
                            return self.parse_coproc();
                        }
                        "function" if self.options.function_keyword => {
                            return self.parse_function_definition();
                        }
                        kw if Token::is_closing_keyword(kw) => {
                            return Err(ParseError::UnexpectedToken {
                                found: keyword_display_name(kw),
                                expected: "a command".to_string(),
                                span: self.lexer.peek()?.span,
                            });
                        }
                        _ => {}
                    }
                }

                // Try POSIX function definition: name() { ... }
                // Phase 1: speculate on the stream to check for name ( )
                if is_lone && is_valid_name(w) {
                    let func_head = self.lexer.speculate(|s| {
                        // No eat_whitespace: line 164 already ate; we're at the Literal
                        let name = match &s.peek()?.token {
                            Token::Literal(w) if is_valid_name(w) => w.clone(),
                            _ => return Ok(None),
                        };
                        let start_span = s.peek()?.span;
                        s.advance()?;
                        s.eat_whitespace()?;
                        if s.peek()?.token != Token::LParen {
                            return Ok(None);
                        }
                        s.advance()?; // consume (
                                      // No eat_whitespace: after ( (operator → LastScanned::Other)
                        if s.peek()?.token != Token::RParen {
                            return Ok(None);
                        }
                        s.advance()?; // consume )
                        Ok(Some((name, start_span)))
                    })?;

                    // Phase 2: if matched, parse the body (committed, no rewind)
                    if let Some((name, start_span)) = func_head {
                        self.skip_linebreak()?;
                        let body = self.parse_compound_command()?;
                        let mut redirects = Vec::new();
                        loop {
                            self.lexer.eat_whitespace()?;
                            if !self.lexer.peek()?.token.is_redirect_start() {
                                break;
                            }
                            redirects.push(self.parse_redirect()?);
                        }
                        let end_span = redirects
                            .last()
                            .map(|r| r.span)
                            .unwrap_or(compound_command_span(&body));
                        return Ok(Expression::FunctionDef(FunctionDef {
                            name,
                            body: Box::new(body),
                            redirects,
                            span: start_span.merge(end_span),
                        }));
                    }
                }

                Ok(Expression::Command(self.parse_command()?))
            }
            Token::LParen => self.parse_compound_expression(),
            Token::BashDblLBracket => self.parse_compound_expression(),
            Token::IoNumber(_) => Ok(Expression::Command(self.parse_command()?)),
            _ if tok.is_redirect_op() => Ok(Expression::Command(self.parse_command()?)),
            _ if tok.is_fragment() => Ok(Expression::Command(self.parse_command()?)),
            _ => Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a command".to_string(),
                span: self.lexer.peek()?.span,
            }),
        }
    }

    pub(super) fn parse_compound_expression(&mut self) -> Result<Expression, ParseError> {
        let body = self.parse_compound_command()?;

        let mut redirects = Vec::new();
        loop {
            self.lexer.eat_whitespace()?;
            if !self.lexer.peek()?.token.is_redirect_start() {
                break;
            }
            redirects.push(self.parse_redirect()?);
        }

        Ok(Expression::Compound { body, redirects })
    }
}
