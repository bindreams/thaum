use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::*;
use super::Parser;

impl<'src> Parser<'src> {
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        self.stream.skip_blanks()?;
        let start_span = self.stream.peek()?.span;
        self.skip_linebreak()?;

        let mut statements = Vec::new();
        while self.stream.peek()?.token != Token::Eof {
            self.parse_list_into(&mut statements)?;
            self.skip_linebreak()?;
        }

        let end_span = self.stream.peek()?.span;
        Ok(Program {
            span: if statements.is_empty() {
                start_span
            } else {
                start_span.merge(end_span)
            },
            statements,
        })
    }

    pub(super) fn parse_list_into(&mut self, out: &mut Vec<Statement>) -> Result<(), ParseError> {
        let mut expr = self.parse_and_or()?;
        let mut span = expr_span(&expr);

        let bodies = self.consume_heredoc_bodies()?;
        if !bodies.is_empty() {
            fill_expression_heredocs(&mut expr, &bodies);
        }

        loop {
            self.stream.skip_blanks()?;
            match self.stream.peek()?.token {
                Token::Semicolon => {
                    out.push(Statement {
                        expression: expr,
                        mode: ExecutionMode::Terminated,
                        span,
                    });
                    self.stream.advance()?;
                    self.skip_linebreak()?;
                    if self.can_start_command()? {
                        expr = self.parse_and_or()?;
                        span = expr_span(&expr);
                        let bodies = self.consume_heredoc_bodies()?;
                        if !bodies.is_empty() {
                            fill_expression_heredocs(&mut expr, &bodies);
                        }
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
                    self.stream.advance()?;
                    self.skip_linebreak()?;
                    if self.can_start_command()? {
                        expr = self.parse_and_or()?;
                        span = expr_span(&expr);
                        let bodies = self.consume_heredoc_bodies()?;
                        if !bodies.is_empty() {
                            fill_expression_heredocs(&mut expr, &bodies);
                        }
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
            self.stream.skip_blanks()?;
            match self.stream.peek()?.token {
                Token::AndIf => {
                    self.stream.advance()?;
                    let bodies = self.consume_heredoc_bodies()?;
                    if !bodies.is_empty() {
                        fill_expression_heredocs(&mut left, &bodies);
                    }
                    self.skip_linebreak()?;
                    let right = self.parse_pipeline()?;
                    left = Expression::And {
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                Token::OrIf => {
                    self.stream.advance()?;
                    let bodies = self.consume_heredoc_bodies()?;
                    if !bodies.is_empty() {
                        fill_expression_heredocs(&mut left, &bodies);
                    }
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
            self.stream.skip_blanks()?;
            let pipe_token = &self.stream.peek()?.token;
            let stderr = match pipe_token {
                Token::Pipe => false,
                Token::BashPipeAmpersand => true,
                _ => break,
            };
            self.stream.advance()?;
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
        self.stream.skip_blanks()?;
        let tok = self.stream.peek()?.token.clone();
        match &tok {
            Token::Literal(w) => {
                let is_lone = {
                    let next = self.stream.peek_at_offset(1)?;
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
                        kw if Self::is_closing_keyword(kw) => {
                            return Err(ParseError::UnexpectedToken {
                                found: keyword_display_name(kw),
                                expected: "a command".to_string(),
                                span: self.stream.peek()?.span,
                            });
                        }
                        _ => {}
                    }
                }

                // Try POSIX function definition: name() { ... }
                // Phase 1: speculate on the stream to check for name ( )
                if is_lone && is_valid_name(w) {
                    let func_head = self.stream.speculate(|s| {
                        s.skip_blanks()?;
                        let name = match &s.peek()?.token {
                            Token::Literal(w) if is_valid_name(w) => w.clone(),
                            _ => return Ok(None),
                        };
                        let start_span = s.peek()?.span;
                        s.advance()?;
                        s.skip_blanks()?;
                        if s.peek()?.token != Token::LParen {
                            return Ok(None);
                        }
                        s.advance()?; // consume (
                        s.skip_blanks()?;
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
                        while self.is_redirect_op()? {
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
            _ if tok.is_fragment() => {
                Ok(Expression::Command(self.parse_command()?))
            }
            _ => Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a command".to_string(),
                span: self.stream.peek()?.span,
            }),
        }
    }

    pub(super) fn parse_compound_expression(&mut self) -> Result<Expression, ParseError> {
        let body = self.parse_compound_command()?;

        let mut redirects = Vec::new();
        while self.is_redirect_op()? {
            redirects.push(self.parse_redirect()?);
        }

        Ok(Expression::Compound { body, redirects })
    }
}

/// Fill heredoc bodies in an expression's redirects.
fn fill_expression_heredocs(expr: &mut Expression, bodies: &[String]) {
    let redirects = match expr {
        Expression::Command(cmd) => &mut cmd.redirects,
        Expression::Compound { redirects, .. } => redirects,
        _ => return,
    };

    let mut body_iter = bodies.iter();
    for redir in redirects.iter_mut() {
        if let RedirectKind::HereDoc { body, .. } = &mut redir.kind {
            if body.is_empty() {
                if let Some(b) = body_iter.next() {
                    *body = b.clone();
                }
            }
        }
    }
}
