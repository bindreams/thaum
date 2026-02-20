use crate::error::LexError;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use super::Lexer;

impl Lexer {
    /// Try to scan a multi-char or single-char operator. Returns `None` if the
    /// current character doesn't start an operator.
    pub(super) fn try_scan_operator(
        &mut self,
        start: usize,
    ) -> Result<Option<SpannedToken>, LexError> {
        let ch = match self.peek_char() {
            Some(c) => c,
            None => return Ok(None),
        };

        let (token, len) = match ch {
            '(' => (Token::LParen, 1),
            ')' => (Token::RParen, 1),
            '&' => {
                if self.chars.peek_at(1) == Some('&') {
                    (Token::AndIf, 2)
                } else if self.options.ampersand_redirect && self.chars.peek_at(1) == Some('>') {
                    if self.chars.peek_at(2) == Some('>') {
                        (Token::BashAppendAllOp, 3)
                    } else {
                        (Token::BashRedirectAllOp, 2)
                    }
                } else {
                    (Token::Ampersand, 1)
                }
            }
            '|' => {
                if self.chars.peek_at(1) == Some('|') {
                    (Token::OrIf, 2)
                } else if self.options.pipe_stderr && self.chars.peek_at(1) == Some('&') {
                    (Token::BashPipeAmpersand, 2)
                } else {
                    (Token::Pipe, 1)
                }
            }
            ';' => {
                if self.chars.peek_at(1) == Some(';') {
                    if self.options.extended_case && self.chars.peek_at(2) == Some('&') {
                        (Token::BashCaseFallThrough, 3)
                    } else {
                        (Token::CaseBreak, 2)
                    }
                } else if self.options.extended_case && self.chars.peek_at(1) == Some('&') {
                    (Token::BashCaseContinue, 2)
                } else {
                    (Token::Semicolon, 1)
                }
            }
            '<' => match self.chars.peek_at(1) {
                Some('&') => (Token::RedirectFromFd, 2),
                Some('>') => (Token::ReadWrite, 2),
                Some('<') => {
                    if self.chars.peek_at(2) == Some('-') {
                        (Token::HereDocStripOp, 3)
                    } else if self.options.here_strings && self.chars.peek_at(2) == Some('<') {
                        (Token::BashHereStringOp, 3)
                    } else {
                        (Token::HereDocOp, 2)
                    }
                }
                Some('(') if self.options.process_substitution && self.last_was_whitespace => {
                    return Ok(None);
                }
                _ => (Token::RedirectFromFile, 1),
            },
            '>' => match self.chars.peek_at(1) {
                Some('>') => (Token::Append, 2),
                Some('&') => (Token::RedirectToFd, 2),
                Some('|') => (Token::Clobber, 2),
                Some('(') if self.options.process_substitution && self.last_was_whitespace => {
                    return Ok(None);
                }
                _ => (Token::RedirectToFile, 1),
            },
            '[' if self.options.double_brackets && self.chars.peek_at(1) == Some('[') => {
                (Token::BashDblLBracket, 2)
            }
            ']' if self.options.double_brackets && self.chars.peek_at(1) == Some(']') => {
                (Token::BashDblRBracket, 2)
            }
            _ => return Ok(None),
        };

        // Advance by `len` characters
        for _ in 0..len {
            self.advance_char();
        }

        Ok(Some(SpannedToken {
            token,
            span: Span::new(start, self.cursor_pos().0),
        }))
    }
}
