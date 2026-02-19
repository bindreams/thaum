use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::is_valid_name;
use super::Parser;

impl<'src> Parser<'src> {
    pub(super) fn parse_command(&mut self) -> Result<Command, ParseError> {
        self.stream.skip_blanks()?;
        let start_span = self.stream.peek()?.span;
        let mut assignments = Vec::new();
        let mut arguments = Vec::new();
        let mut redirects = Vec::new();
        let mut end_span = start_span;

        loop {
            if self.is_redirect_op()? {
                let redir = self.parse_redirect()?;
                end_span = redir.span;
                redirects.push(redir);
                continue;
            }

            // Assignment detection: first Literal in a word contains '='
            if let Token::Literal(ref w) = self.stream.peek()?.token {
                if let Some(eq_pos) = w.find('=') {
                    let name = &w[..eq_pos];
                    if is_valid_name(name) && !name.is_empty() {
                        let value_prefix = w[eq_pos + 1..].to_string();
                        let name = name.to_string();
                        let word_span = self.stream.peek()?.span;
                        self.stream.advance()?; // consume the Literal("name=value...")

                        // Array assignment: name=( ... ) (Bash)
                        if value_prefix.is_empty()
                            && self.options.arrays
                            && self.stream.peek()?.token == Token::LParen
                        {
                            self.stream.advance()?; // consume (
                            let mut elements = Vec::new();
                            loop {
                                self.skip_linebreak()?;
                                if self.stream.peek()?.token == Token::RParen {
                                    break;
                                }
                                if self.is_word()? {
                                    if let Some(w) = self.collect_word()? {
                                        elements.push(w);
                                    }
                                } else {
                                    break;
                                }
                            }
                            let rparen_span = self.stream.peek()?.span;
                            self.stream.advance()?; // consume )
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
                        continue;
                    }
                }
            }

            break;
        }

        if self.is_word()? {
            end_span = self.stream.peek()?.span;
            if let Some(arg) = self.collect_argument()? {
                arguments.push(arg);
            }

            loop {
                self.stream.skip_blanks()?;

                if self.is_redirect_op()? {
                    let redir = self.parse_redirect()?;
                    end_span = redir.span;
                    redirects.push(redir);
                    continue;
                }

                if self.is_word()? {
                    end_span = self.stream.peek()?.span;
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
        self.stream.skip_blanks()?;
        let start_span = self.stream.peek()?.span;

        let fd = if let Token::IoNumber(n) = self.stream.peek()?.token {
            self.stream.advance()?;
            Some(n)
        } else {
            None
        };

        let op_token = self.stream.peek()?.token.clone();
        self.stream.advance()?;

        match op_token {
            Token::HereDocOp | Token::HereDocStripOp => {
                return self.parse_here_redirect(fd, op_token == Token::HereDocStripOp, start_span);
            }
            Token::BashHereStringOp => {
                return self.parse_here_string_redirect(fd, start_span);
            }
            _ => {}
        }

        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a filename for redirection".to_string(),
                span: self.stream.peek()?.span,
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
        self.stream.skip_blanks()?;
        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a here-document delimiter".to_string(),
                span: self.stream.peek()?.span,
            });
        }

        // The heredoc delimiter was scanned as a single raw Literal by the lexer
        let raw_delimiter = match &self.stream.peek()?.token {
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
        let delim_span = self.stream.peek()?.span;

        let (delimiter, quoted) = crate::lexer::heredoc::strip_heredoc_quotes(&raw_delimiter);

        // Consume the delimiter token(s)
        if matches!(self.stream.peek()?.token, Token::Literal(_)) {
            self.stream.advance()?;
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
        self.stream.skip_blanks()?;
        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a word for here-string".to_string(),
                span: self.stream.peek()?.span,
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
