use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::{make_argument, make_word};
use super::Parser;

impl<'src> Parser<'src> {
    pub(super) fn parse_command(&mut self) -> Result<Command, ParseError> {
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

            if let Token::Word(ref w) = self.stream.peek()?.token {
                if let Some(eq_pos) = w.find('=') {
                    let name = &w[..eq_pos];
                    if super::helpers::is_valid_name(name) && !name.is_empty() {
                        let value_str = w[eq_pos + 1..].to_string();
                        let name = name.to_string();
                        let word_span = self.stream.peek()?.span;
                        self.stream.advance()?;

                        // Array assignment: name=( ... ) (Bash)
                        if value_str.is_empty()
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
                                    let ws = self.stream.peek()?.span;
                                    if let Token::Word(s) = &self.stream.peek()?.token {
                                        elements.push(make_word(s.clone(), ws, &self.options));
                                    }
                                    self.stream.advance()?;
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
                            assignments.push(Assignment {
                                name,
                                value: AssignmentValue::Scalar(
                                    make_word(value_str, word_span, &self.options),
                                ),
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
            let w = self.stream.peek()?.token.clone();
            end_span = self.stream.peek()?.span;
            if let Token::Word(s) = w {
                arguments.push(make_argument(s, self.stream.peek()?.span, &self.options));
            }
            self.stream.advance()?;

            loop {
                if self.is_redirect_op()? {
                    let redir = self.parse_redirect()?;
                    end_span = redir.span;
                    redirects.push(redir);
                    continue;
                }

                if self.is_word()? {
                    let span = self.stream.peek()?.span;
                    end_span = span;
                    if let Token::Word(s) = &self.stream.peek()?.token {
                        arguments.push(make_argument(s.clone(), span, &self.options));
                    }
                    self.stream.advance()?;
                    continue;
                }

                break;
            }
        }

        // NOTE: Heredoc body consumption is NOT done here. It's done in
        // parse_list_into after the full statement is parsed, because the
        // Newline + HereDocBody tokens follow the command and must be consumed
        // as part of statement termination (like a semicolon).

        Ok(Command {
            assignments,
            arguments,
            redirects,
            span: start_span.merge(end_span),
        })
    }

    pub(super) fn parse_redirect(&mut self) -> Result<Redirect, ParseError> {
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

        let target_str = match &self.stream.peek()?.token {
            Token::Word(s) => s.clone(),
            _ => unreachable!(),
        };
        let target_span = self.stream.peek()?.span;
        let target = make_word(target_str, target_span, &self.options);

        self.stream.advance()?;

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

    /// Parse a here-document redirect (`<<` or `<<-`).
    ///
    /// The lexer has already auto-registered the pending heredoc when it saw
    /// the delimiter word. We just need to record the redirect with an empty
    /// body — `fill_heredoc_bodies` will fill it in from `HereDocBody` tokens.
    fn parse_here_redirect(
        &mut self,
        fd: Option<i32>,
        strip_tabs: bool,
        start_span: crate::span::Span,
    ) -> Result<Redirect, ParseError> {
        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a here-document delimiter".to_string(),
                span: self.stream.peek()?.span,
            });
        }

        let raw_delimiter = match &self.stream.peek()?.token {
            Token::Word(s) => s.clone(),
            _ => unreachable!(),
        };
        let delim_span = self.stream.peek()?.span;

        // TODO: strip_heredoc_quotes is called twice — once in the lexer (when
        // auto-registering the heredoc) and once here for the AST. The fix: include
        // `quoted` and `delimiter` in the HereDocBody token so the parser doesn't
        // need to recompute them.
        let (delimiter, quoted) = crate::lexer::heredoc::strip_heredoc_quotes(&raw_delimiter);

        self.stream.advance()?;

        Ok(Redirect {
            fd,
            kind: RedirectKind::HereDoc {
                delimiter,
                body: String::new(), // filled in by fill_heredoc_bodies
                strip_tabs,
                quoted,
            },
            span: start_span.merge(delim_span),
        })
    }

    /// Parse `<<< word` (here-string).
    fn parse_here_string_redirect(
        &mut self,
        fd: Option<i32>,
        start_span: crate::span::Span,
    ) -> Result<Redirect, ParseError> {
        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a word for here-string".to_string(),
                span: self.stream.peek()?.span,
            });
        }

        let target_str = match &self.stream.peek()?.token {
            Token::Word(s) => s.clone(),
            _ => unreachable!(),
        };
        let target_span = self.stream.peek()?.span;
        let target = make_word(target_str, target_span, &self.options);

        self.stream.advance()?;

        Ok(Redirect {
            fd,
            kind: RedirectKind::BashHereString(target),
            span: start_span.merge(target_span),
        })
    }
}
