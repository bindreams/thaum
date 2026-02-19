use crate::error::LexError;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use super::Lexer;

impl<'src> Lexer<'src> {
    /// Try to scan a multi-char or single-char operator. Returns `None` if the
    /// current character doesn't start an operator.
    pub(super) fn try_scan_operator(
        &mut self,
        start: usize,
    ) -> Result<Option<SpannedToken>, LexError> {
        let ch = match self.cursor.peek() {
            Some(c) => c,
            None => return Ok(None),
        };

        let (token, len) = match ch {
            '(' => (Token::LParen, 1),
            ')' => (Token::RParen, 1),
            '&' => {
                if self.cursor.peek_second() == Some('&') {
                    (Token::AndIf, 2)
                } else if self.options.ampersand_redirect && self.cursor.peek_second() == Some('>')
                {
                    // Check for &>> vs &>
                    let saved_pos = self.cursor.pos;
                    self.cursor.advance(); // &
                    self.cursor.advance(); // >
                    if self.cursor.peek() == Some('>') {
                        self.cursor.pos = saved_pos;
                        (Token::BashAppendAllOp, 3)
                    } else {
                        self.cursor.pos = saved_pos;
                        (Token::BashRedirectAllOp, 2)
                    }
                } else {
                    (Token::Ampersand, 1)
                }
            }
            '|' => {
                if self.cursor.peek_second() == Some('|') {
                    (Token::OrIf, 2)
                } else if self.options.pipe_stderr && self.cursor.peek_second() == Some('&') {
                    (Token::BashPipeAmpersand, 2)
                } else {
                    (Token::Pipe, 1)
                }
            }
            ';' => {
                if self.cursor.peek_second() == Some(';') {
                    if self.options.extended_case {
                        // Check for ;;& (three chars)
                        let saved_pos = self.cursor.pos;
                        self.cursor.advance(); // ;
                        self.cursor.advance(); // ;
                        if self.cursor.peek() == Some('&') {
                            self.cursor.pos = saved_pos;
                            (Token::BashCaseFallThrough, 3)
                        } else {
                            self.cursor.pos = saved_pos;
                            (Token::CaseBreak, 2)
                        }
                    } else {
                        (Token::CaseBreak, 2)
                    }
                } else if self.options.extended_case && self.cursor.peek_second() == Some('&') {
                    (Token::BashCaseContinue, 2)
                } else {
                    (Token::Semicolon, 1)
                }
            }
            '<' => match self.cursor.peek_second() {
                Some('&') => (Token::RedirectFromFd, 2),
                Some('>') => (Token::ReadWrite, 2),
                Some('<') => {
                    // Could be <<, <<-, or <<< (here-string)
                    let saved_pos = self.cursor.pos;
                    self.cursor.advance(); // consume first <
                    self.cursor.advance(); // consume second <
                    if self.cursor.peek() == Some('-') {
                        self.cursor.pos = saved_pos;
                        (Token::HereDocStripOp, 3)
                    } else if self.options.here_strings && self.cursor.peek() == Some('<') {
                        self.cursor.pos = saved_pos;
                        (Token::BashHereStringOp, 3)
                    } else {
                        self.cursor.pos = saved_pos;
                        (Token::HereDocOp, 2)
                    }
                }
                Some('(') if self.options.process_substitution && self.last_was_blank => {
                    return Ok(None);
                }
                _ => (Token::RedirectFromFile, 1),
            },
            '>' => match self.cursor.peek_second() {
                Some('>') => (Token::Append, 2),
                Some('&') => (Token::RedirectToFd, 2),
                Some('|') => (Token::Clobber, 2),
                Some('(') if self.options.process_substitution && self.last_was_blank => {
                    return Ok(None);
                }
                _ => (Token::RedirectToFile, 1),
            },
            '[' if self.options.double_brackets && self.cursor.peek_second() == Some('[') => {
                (Token::BashDblLBracket, 2)
            }
            ']' if self.options.double_brackets && self.cursor.peek_second() == Some(']') => {
                (Token::BashDblRBracket, 2)
            }
            _ => return Ok(None),
        };

        // Advance by `len` characters
        for _ in 0..len {
            self.cursor.advance();
        }

        Ok(Some(SpannedToken {
            token,
            span: Span::new(start, self.cursor.pos().0),
        }))
    }
}
