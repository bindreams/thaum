use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::*;
use super::Parser;

impl Parser {
    pub(super) fn parse_coproc(&mut self) -> Result<Expression, ParseError> {
        self.lexer.skip_blanks()?;
        let start_span = self.lexer.peek()?.span;
        self.lexer.advance()?; // consume "coproc"

        // If the next token starts a compound command, there's no name
        self.lexer.skip_blanks()?;
        let tok = self.lexer.peek()?.token.clone();
        if tok.is_compound_start(&self.lexer.peek_at_offset(1)?.token, self.options.select) {
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
        if self.lexer.peek()?.token.is_fragment() {
            // Collect the first word as a plain string for the name candidate
            let first_word = self.collect_word()?.unwrap();
            let word_span = first_word.span;
            // Extract raw name from the first word (for coproc naming)
            let saved_name = first_word.parts.iter().map(|f| match f {
                Fragment::Literal(s) => s.clone(),
                _ => String::new(),
            }).collect::<String>();

            // If now we see a compound command start, the word was the name
            self.lexer.skip_blanks()?;
            let tok = self.lexer.peek()?.token.clone();
            if tok.is_compound_start(&self.lexer.peek_at_offset(1)?.token, self.options.select) {
                let body_expr = self.parse_compound_expression()?;
                let span = start_span.merge(expr_span(&body_expr));
                return Ok(Expression::Compound {
                    body: CompoundCommand::BashCoproc {
                        name: Some(saved_name),
                        body: Box::new(body_expr),
                        span,
                    },
                    redirects: Vec::new(),
                });
            }

            // Otherwise, it's a simple command starting with the collected word
            let mut arguments: Vec<Argument> = vec![Argument::Word(first_word)];
            let mut redirects = Vec::new();

            loop {
                self.lexer.skip_blanks()?;
                if self.lexer.peek()?.token.is_redirect_start() {
                    redirects.push(self.parse_redirect()?);
                } else if self.lexer.peek()?.token.is_fragment() {
                    if let Some(arg) = self.collect_argument()? {
                        arguments.push(arg);
                    }
                } else {
                    break;
                }
            }

            let cmd_span =
                start_span.merge(arguments.last().map(|a| argument_span(a)).unwrap_or(word_span));
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
            found: self.lexer.peek()?.token.display_name().to_string(),
            expected: "a command after 'coproc'".to_string(),
            span: self.lexer.peek()?.span,
        })
    }

    pub(super) fn parse_select_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        self.lexer.skip_blanks()?;
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("select")?;

        self.lexer.skip_blanks()?;
        let var_name = match &self.lexer.peek()?.token {
            Token::Literal(s) => s.clone(),
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: self.lexer.peek()?.token.display_name().to_string(),
                    expected: "a variable name".to_string(),
                    span: self.lexer.peek()?.span,
                });
            }
        };
        self.lexer.advance()?;
        self.skip_linebreak()?;

        let tok = self.lexer.peek()?.token.clone();
        let words = if tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "in") {
            self.lexer.advance()?;
            let mut word_list = Vec::new();
            loop {
                self.lexer.skip_blanks()?;
                if !self.lexer.peek()?.token.is_fragment() { break; }
                if let Some(w) = self.collect_word()? {
                    word_list.push(w);
                }
            }
            if self.lexer.peek()?.token == Token::Semicolon {
                self.lexer.advance()?;
            }
            self.skip_linebreak()?;
            Some(word_list)
        } else {
            if self.lexer.peek()?.token == Token::Semicolon {
                self.lexer.advance()?;
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

    pub(super) fn parse_function_definition(&mut self) -> Result<Expression, ParseError> {
        self.lexer.skip_blanks()?;
        let start_span = self.lexer.peek()?.span;

        let tok = self.lexer.peek()?.token.clone();
        let has_function_keyword = tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "function");
        if has_function_keyword {
            self.lexer.advance()?;
        }

        self.lexer.skip_blanks()?;
        let name = match &self.lexer.peek()?.token {
            Token::Literal(s) => s.clone(),
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: self.lexer.peek()?.token.display_name().to_string(),
                    expected: "a function name".to_string(),
                    span: self.lexer.peek()?.span,
                });
            }
        };

        if has_function_keyword {
            self.lexer.advance()?;
            self.lexer.skip_blanks()?;
            if self.lexer.peek()?.token == Token::LParen {
                self.lexer.advance()?;
                self.expect(&Token::RParen)?;
            }
        } else {
            self.lexer.advance()?;
            self.expect(&Token::LParen)?;
            self.expect(&Token::RParen)?;
        }
        self.skip_linebreak()?;

        let body = self.parse_compound_command()?;

        let mut redirects = Vec::new();
        loop {
            self.lexer.skip_blanks()?;
            if !self.lexer.peek()?.token.is_redirect_start() { break; }
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
