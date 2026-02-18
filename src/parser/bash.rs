use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::*;
use super::Parser;

impl<'src> Parser<'src> {
    pub(super) fn parse_coproc(&mut self) -> Result<Expression, ParseError> {
        let start_span = self.stream.peek()?.span;
        self.stream.advance()?; // consume "coproc"

        // If the next token starts a compound command, there's no name
        if self.is_compound_start()? {
            let body_expr = self.parse_compound_expression()?;
            let span = start_span.merge(expr_span(&body_expr));
            return Ok(Expression::Compound {
                body: CompoundCommand::BashCoproc {
                    name: None,
                    body: Box::new(body_expr),
                    span,
                },
                redirects: Vec::new(),
            });
        }

        // Next token should be a word -- might be a name or the start of a simple command
        if let Token::Word(word) = &self.stream.peek()?.token {
            let saved_word = word.clone();
            let word_span = self.stream.peek()?.span;
            self.stream.advance()?;

            // If now we see a compound command start, the word was the name
            if self.is_compound_start()? {
                let body_expr = self.parse_compound_expression()?;
                let span = start_span.merge(expr_span(&body_expr));
                return Ok(Expression::Compound {
                    body: CompoundCommand::BashCoproc {
                        name: Some(saved_word),
                        body: Box::new(body_expr),
                        span,
                    },
                    redirects: Vec::new(),
                });
            }

            // Otherwise, it's a simple command starting with saved_word
            // Build a command manually: saved_word is the command name,
            // continue parsing arguments and redirects
            let mut arguments = vec![make_argument(saved_word, word_span, &self.options)];
            let mut redirects = Vec::new();

            loop {
                if self.is_redirect_op()? {
                    redirects.push(self.parse_redirect()?);
                } else if self.is_word()? {
                    let wspan = self.stream.peek()?.span;
                    if let Token::Word(s) = &self.stream.peek()?.token {
                        arguments.push(make_argument(s.clone(), wspan, &self.options));
                    }
                    self.stream.advance()?;
                } else {
                    break;
                }
            }

            self.fill_heredoc_bodies(&mut redirects)?;

            let cmd_span =
                start_span.merge(arguments.last().map(argument_span).unwrap_or(word_span));
            let body_expr = Expression::Command(Command {
                assignments: Vec::new(),
                arguments,
                redirects: Vec::new(),
                span: cmd_span,
            });
            return Ok(Expression::Compound {
                body: CompoundCommand::BashCoproc {
                    name: None,
                    body: Box::new(body_expr),
                    span: cmd_span,
                },
                redirects,
            });
        }

        Err(ParseError::UnexpectedToken {
            found: self.stream.peek()?.token.display_name().to_string(),
            expected: "a command after 'coproc'".to_string(),
            span: self.stream.peek()?.span,
        })
    }

    pub(super) fn parse_select_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.stream.peek()?.span;
        self.expect_keyword("select")?;

        let var_name = match &self.stream.peek()?.token {
            Token::Word(s) => s.clone(),
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: self.stream.peek()?.token.display_name().to_string(),
                    expected: "a variable name".to_string(),
                    span: self.stream.peek()?.span,
                });
            }
        };
        self.stream.advance()?;
        self.skip_linebreak()?;

        let words = if is_keyword(&self.stream.peek()?.token, "in") {
            self.stream.advance()?;
            let mut word_list = Vec::new();
            while self.is_word()? {
                let span = self.stream.peek()?.span;
                if let Token::Word(s) = &self.stream.peek()?.token {
                    word_list.push(make_word(s.clone(), span, &self.options));
                }
                self.stream.advance()?;
            }
            if self.stream.peek()?.token == Token::Semicolon {
                self.stream.advance()?;
            }
            self.skip_linebreak()?;
            Some(word_list)
        } else {
            if self.stream.peek()?.token == Token::Semicolon {
                self.stream.advance()?;
            }
            self.skip_linebreak()?;
            None
        };

        self.expect_closing_keyword("do", "select", start_span)?;
        let body = self.parse_required_compound_list("do body")?;
        let done_tok = self.expect_closing_keyword("done", "select", start_span)?;

        Ok(CompoundCommand::BashSelectClause {
            variable: var_name,
            words,
            body,
            span: start_span.merge(done_tok.span),
        })
    }

    /// Speculatively try a parse. If the closure returns `Some(value)`,
    /// the parse succeeded and the stream position is kept. If it returns
    /// `None`, the stream is rewound to where it was before the attempt.
    pub(super) fn try_parse<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<Option<T>, ParseError>,
    ) -> Result<Option<T>, ParseError> {
        let saved = self.stream.checkpoint();
        match f(self)? {
            Some(v) => {
                self.stream.release(saved);
                Ok(Some(v))
            }
            None => {
                self.stream.rewind(saved);
                Ok(None)
            }
        }
    }

    pub(super) fn parse_function_definition(&mut self) -> Result<Expression, ParseError> {
        let start_span = self.stream.peek()?.span;

        // Handle `function` keyword (Bash)
        let has_function_keyword = is_keyword(&self.stream.peek()?.token, "function");
        if has_function_keyword {
            self.stream.advance()?;
        }

        let name = match &self.stream.peek()?.token {
            Token::Word(s) => s.clone(),
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: self.stream.peek()?.token.display_name().to_string(),
                    expected: "a function name".to_string(),
                    span: self.stream.peek()?.span,
                });
            }
        };

        if has_function_keyword {
            // `function name` -- parens are optional
            self.stream.advance()?;
            if self.stream.peek()?.token == Token::LParen {
                self.stream.advance()?; // (
                self.expect(&Token::RParen)?; // )
            }
            // With the new design, `{` always comes as Word("{"),
            // and the parser checks for it directly via parse_compound_command.
        } else {
            // POSIX: `name ()` -- parens required
            self.stream.advance()?;
            self.expect(&Token::LParen)?;
            self.expect(&Token::RParen)?;
        }
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

        Ok(Expression::FunctionDef(FunctionDef {
            name,
            body: Box::new(body),
            redirects,
            span: start_span.merge(end_span),
        }))
    }
}
