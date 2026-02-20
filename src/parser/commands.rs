use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::is_valid_name;
use super::Parser;

impl Parser {
    pub(super) fn parse_command(&mut self) -> Result<Command, ParseError> {
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_command"
        );
        let start_span = self.lexer.peek()?.span;
        let mut assignments = Vec::new();
        let mut arguments = Vec::new();
        let mut redirects = Vec::new();
        let mut end_span = start_span;

        loop {
            if self.lexer.peek()?.token.is_redirect_start() {
                let redir = self.parse_redirect()?;
                end_span = redir.span;
                redirects.push(redir);
                self.lexer.eat_whitespace()?;
                continue;
            }

            // Assignment detection: first Literal in a word contains '='
            if let Token::Literal(ref w) = self.lexer.peek()?.token {
                if let Some(eq_pos) = w.find('=') {
                    let name = &w[..eq_pos];
                    if is_valid_name(name) && !name.is_empty() {
                        let value_prefix = w[eq_pos + 1..].to_string();
                        let name = name.to_string();
                        let word_span = self.lexer.peek()?.span;
                        self.lexer.advance()?; // consume the Literal("name=value...")

                        // Array assignment: name=( ... ) (Bash)
                        if value_prefix.is_empty()
                            && self.options.arrays
                            && self.lexer.peek()?.token == Token::LParen
                        {
                            self.lexer.advance()?; // consume (
                            let mut elements = Vec::new();
                            loop {
                                self.skip_linebreak()?;
                                if self.lexer.peek()?.token == Token::RParen {
                                    break;
                                }
                                self.lexer.eat_whitespace()?;
                                if self.lexer.peek()?.token.is_fragment() {
                                    if let Some(w) = self.collect_word()? {
                                        elements.push(w);
                                    }
                                } else {
                                    break;
                                }
                            }
                            let rparen_span = self.lexer.peek()?.span;
                            self.lexer.advance()?; // consume )
                            let arr_span = word_span.merge(rparen_span);
                            assignments.push(Assignment {
                                name,
                                value: AssignmentValue::BashArray(elements),
                                span: arr_span,
                            });
                            end_span = rparen_span;
                        } else {
                            let value_word = self.collect_assignment_value(&value_prefix, word_span)?;
                            assignments.push(Assignment {
                                name,
                                value: AssignmentValue::Scalar(value_word),
                                span: word_span,
                            });
                            end_span = word_span;
                        }
                        self.lexer.eat_whitespace()?;
                        continue;
                    }
                }
            }

            break;
        }

        self.lexer.eat_whitespace()?;
        if self.lexer.peek()?.token.is_fragment() {
            end_span = self.lexer.peek()?.span;
            if let Some(arg) = self.collect_argument()? {
                arguments.push(arg);
            }

            loop {
                self.lexer.eat_whitespace()?;

                if self.lexer.peek()?.token.is_redirect_start() {
                    let redir = self.parse_redirect()?;
                    end_span = redir.span;
                    redirects.push(redir);
                    continue;
                }

                if self.lexer.peek()?.token.is_fragment() {
                    end_span = self.lexer.peek()?.span;
                    if let Some(arg) = self.collect_argument()? {
                        arguments.push(arg);
                    }
                    continue;
                }

                break;
            }
        }

        Ok(Command {
            assignments,
            arguments,
            redirects,
            span: start_span.merge(end_span),
        })
    }

    pub(super) fn parse_redirect(&mut self) -> Result<Redirect, ParseError> {
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_redirect"
        );
        let start_span = self.lexer.peek()?.span;

        let fd = if let Token::IoNumber(n) = self.lexer.peek()?.token {
            self.lexer.advance()?;
            Some(n)
        } else {
            None
        };

        let op_token = self.lexer.peek()?.token.clone();
        self.lexer.advance()?;

        match op_token {
            Token::HereDocOp | Token::HereDocStripOp => {
                return self.parse_here_redirect(fd, op_token == Token::HereDocStripOp, start_span);
            }
            Token::BashHereStringOp => {
                return self.parse_here_string_redirect(fd, start_span);
            }
            _ => {}
        }

        self.lexer.eat_whitespace()?;
        if !self.lexer.peek()?.token.is_fragment() {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a filename for redirection".to_string(),
                span: self.lexer.peek()?.span,
            });
        }

        let target = self.collect_word()?.unwrap();
        let target_span = target.span;

        let kind = match op_token {
            Token::RedirectFromFile => RedirectKind::Input(target),
            Token::RedirectToFile => RedirectKind::Output(target),
            Token::Append => RedirectKind::Append(target),
            Token::Clobber => RedirectKind::Clobber(target),
            Token::ReadWrite => RedirectKind::ReadWrite(target),
            Token::RedirectFromFd => RedirectKind::DupInput(target),
            Token::RedirectToFd => RedirectKind::DupOutput(target),
            Token::BashRedirectAllOp => RedirectKind::BashOutputAll(target),
            Token::BashAppendAllOp => RedirectKind::BashAppendAll(target),
            _ => unreachable!(),
        };

        Ok(Redirect {
            fd,
            kind,
            span: start_span.merge(target_span),
        })
    }

    fn parse_here_redirect(
        &mut self,
        fd: Option<i32>,
        strip_tabs: bool,
        start_span: crate::span::Span,
    ) -> Result<Redirect, ParseError> {
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_here_redirect"
        );
        if !self.lexer.peek()?.token.is_fragment() {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a here-document delimiter".to_string(),
                span: self.lexer.peek()?.span,
            });
        }

        // The heredoc delimiter was scanned as a single raw Literal by the lexer
        let raw_delimiter = match &self.lexer.peek()?.token {
            Token::Literal(s) => s.clone(),
            _ => {
                // If it's some other fragment, collect the whole word as raw text
                let w = self.collect_word()?.unwrap();
                // Reconstruct raw text from fragments (approximation)
                w.parts.iter().map(|f| match f {
                    Fragment::Literal(s) => s.as_str(),
                    _ => "",
                }).collect::<String>()
            }
        };
        let delim_span = self.lexer.peek()?.span;

        let (delimiter, quoted) = crate::lexer::heredoc::strip_heredoc_quotes(&raw_delimiter);

        // Consume the delimiter token(s)
        if matches!(self.lexer.peek()?.token, Token::Literal(_)) {
            self.lexer.advance()?;
        }

        Ok(Redirect {
            fd,
            kind: RedirectKind::HereDoc {
                delimiter,
                body: String::new(),
                strip_tabs,
                quoted,
            },
            span: start_span.merge(delim_span),
        })
    }

    fn parse_here_string_redirect(
        &mut self,
        fd: Option<i32>,
        start_span: crate::span::Span,
    ) -> Result<Redirect, ParseError> {
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_here_string_redirect"
        );
        if !self.lexer.peek()?.token.is_fragment() {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a word for here-string".to_string(),
                span: self.lexer.peek()?.span,
            });
        }

        let target = self.collect_word()?.unwrap();
        let target_span = target.span;

        Ok(Redirect {
            fd,
            kind: RedirectKind::BashHereString(target),
            span: start_span.merge(target_span),
        })
    }
}
