use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::*;
use super::Parser;

impl<'src> Parser<'src> {
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
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

    /// Parse a `;`/`&`-separated list of statements.
    ///
    /// After parsing each expression, consumes any heredoc bodies that belong
    /// to it (Newline + HereDocBody tokens). This ensures the statement owns
    /// all its heredoc content before the next statement begins.
    pub(super) fn parse_list_into(&mut self, out: &mut Vec<Statement>) -> Result<(), ParseError> {
        let mut expr = self.parse_and_or()?;
        let mut span = expr_span(&expr);

        // Consume heredoc bodies that belong to this expression
        let bodies = self.consume_heredoc_bodies()?;
        if !bodies.is_empty() {
            fill_expression_heredocs(&mut expr, &bodies);
        }

        loop {
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

    /// and_or: pipeline ((AND_IF | OR_IF) linebreak pipeline)*
    pub(super) fn parse_and_or(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_pipeline()?;

        loop {
            match self.stream.peek()?.token {
                Token::AndIf => {
                    self.stream.advance()?;
                    self.skip_linebreak()?;
                    let right = self.parse_pipeline()?;
                    left = Expression::And {
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                Token::OrIf => {
                    self.stream.advance()?;
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

    /// pipeline: [!] pipe_sequence
    fn parse_pipeline(&mut self) -> Result<Expression, ParseError> {
        let negated = self.eat_keyword("!")?;

        let mut left = self.parse_leaf_expression()?;

        loop {
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

    /// A leaf expression: command | compound_command redirect_list? | function_definition
    fn parse_leaf_expression(&mut self) -> Result<Expression, ParseError> {
        let tok = self.stream.peek()?.token.clone();
        match &tok {
            Token::Word(w) => {
                match w.as_str() {
                    "if" | "while" | "until" | "for" | "case" | "{" => {
                        self.parse_compound_expression()
                    }
                    "select" if self.options.select => self.parse_compound_expression(),
                    "coproc" if self.options.coproc => self.parse_coproc(),
                    "function" if self.options.function_keyword => self.parse_function_definition(),
                    kw if Self::is_closing_keyword(kw) => Err(ParseError::UnexpectedToken {
                        found: keyword_display_name(kw),
                        expected: "a command".to_string(),
                        span: self.stream.peek()?.span,
                    }),
                    _ => {
                        // Try POSIX function definition: name() { ... }
                        // Uses speculative parsing: peek at name + `(`, rewind if not.
                        if let Some(func) = self.try_parse(|p| {
                            let name = match &p.stream.peek()?.token {
                                Token::Word(w) if is_valid_name(w) => w.clone(),
                                _ => return Ok(None),
                            };
                            let start_span = p.stream.peek()?.span;
                            p.stream.advance()?; // consume name
                            if p.stream.peek()?.token != Token::LParen {
                                return Ok(None); // not name( -> rewind
                            }
                            p.stream.advance()?; // consume (
                            p.expect(&Token::RParen)?; // expect )
                            p.skip_linebreak()?;
                            let body = p.parse_compound_command()?;
                            let mut redirects = Vec::new();
                            while p.is_redirect_op()? {
                                redirects.push(p.parse_redirect()?);
                            }
                            let end_span = redirects
                                .last()
                                .map(|r| r.span)
                                .unwrap_or(compound_command_span(&body));
                            Ok(Some(Expression::FunctionDef(FunctionDef {
                                name,
                                body: Box::new(body),
                                redirects,
                                span: start_span.merge(end_span),
                            })))
                        })? {
                            Ok(func)
                        } else {
                            Ok(Expression::Command(self.parse_command()?))
                        }
                    }
                }
            }
            Token::LParen => self.parse_compound_expression(),
            Token::BashDblLBracket => self.parse_compound_expression(),
            Token::IoNumber(_) => Ok(Expression::Command(self.parse_command()?)),
            _ if tok.is_redirect_op() => Ok(Expression::Command(self.parse_command()?)),
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

        // NOTE: Heredoc body consumption handled by parse_list_into

        Ok(Expression::Compound { body, redirects })
    }
}

/// Fill heredoc bodies in an expression's redirects.
///
/// Finds all `RedirectKind::HereDoc` with empty bodies and fills them
/// from the provided body strings, in order.
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
