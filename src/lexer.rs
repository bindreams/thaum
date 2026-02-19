mod cursor;
pub(crate) mod heredoc;
mod operators;
mod word_scan;

use std::collections::VecDeque;

use crate::dialect::ParseOptions;
use crate::error::LexError;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use cursor::Cursor;
use heredoc::PendingHereDoc;

/// The shell lexer.
pub struct Lexer<'src> {
    cursor: Cursor<'src>,
    pending_heredocs: Vec<PendingHereDoc>,
    /// When true, the next Word token is a heredoc delimiter.
    expecting_heredoc_delimiter: bool,
    /// Whether the current pending heredoc delimiter should strip tabs.
    pending_strip_tabs: bool,
    // TODO: expecting_heredoc_delimiter and pending_heredocs are lexer state not
    // covered by TokenStream checkpoint/rewind. If speculative parsing (try_parse)
    // ever lexes past `<<` or a delimiter word (advancing past the buffer), these
    // flags get set and won't be cleared on rewind. Currently safe because try_parse
    // only peeks 1-2 tokens for function definitions.
    /// Extra tokens to emit before scanning more source (heredoc bodies, newlines).
    queued_tokens: VecDeque<SpannedToken>,
    options: ParseOptions,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str, options: ParseOptions) -> Self {
        Lexer {
            cursor: Cursor::new(source),
            pending_heredocs: Vec::new(),
            expecting_heredoc_delimiter: false,
            pending_strip_tabs: false,
            queued_tokens: VecDeque::new(),
            options,
        }
    }

    /// Get the next token from the source.
    ///
    /// The lexer is context-free — it never promotes words to reserved word
    /// tokens. That's the parser's job. The lexer produces `Word`,
    /// `IoNumber`, operators, `Newline`, `HereDocBody`, and `Eof`.
    pub fn next_token(&mut self) -> Result<SpannedToken, LexError> {
        // Return queued tokens first (heredoc bodies + deferred newline)
        if !self.queued_tokens.is_empty() {
            return Ok(self.queued_tokens.pop_front().unwrap());
        }

        let pos_before_blanks = self.cursor.pos().0;
        self.skip_blanks();
        self.skip_comment();
        let preceded_by_blank = self.cursor.pos().0 > pos_before_blanks;

        let start = self.cursor.pos().0;

        // EOF
        if self.cursor.is_eof() {
            // queued_tokens should always be drained before we reach EOF scanning.
            // pending_heredocs / expecting_heredoc_delimiter may be set if the input
            // is truncated (e.g., `<<` without a delimiter) — that's a user error,
            // not an internal invariant violation.
            debug_assert!(
                self.queued_tokens.is_empty(),
                "EOF reached with {} queued tokens undrained",
                self.queued_tokens.len()
            );
            return Ok(SpannedToken {
                token: Token::Eof,
                span: Span::empty(start),
            });
        }

        let ch = self.cursor.peek().unwrap();

        // Newline — also triggers heredoc body reading
        if ch == '\n' {
            self.cursor.advance();
            let newline_span = Span::new(start, start + 1);

            if !self.pending_heredocs.is_empty() {
                // Read all pending heredoc bodies
                let pending = std::mem::take(&mut self.pending_heredocs);
                for heredoc in &pending {
                    let body = self.read_single_heredoc(&heredoc.delimiter, heredoc.strip_tabs)?;
                    // Queue: Newline first, then HereDocBody tokens
                    // TODO: The span should cover the actual body content in the source,
                    // not the newline. Track the cursor position before/after reading
                    // the body and use that for the span.
                    self.queued_tokens.push_back(SpannedToken {
                        token: Token::HereDocBody(body),
                        span: newline_span, // wrong — should be the body's span
                    });
                }
                // Return the Newline immediately, bodies are queued
                return Ok(SpannedToken {
                    token: Token::Newline,
                    span: newline_span,
                });
            }

            return Ok(SpannedToken {
                token: Token::Newline,
                span: newline_span,
            });
        }

        // Operators
        if let Some(tok) = self.try_scan_operator(start, preceded_by_blank)? {
            // Track heredoc operators — next word is the delimiter
            // TODO: If `<<` is at EOF or followed by a newline (no delimiter word),
            // the flag stays set and is never cleared. This produces a generic parse
            // error but leaves lexer state unclean. The fix: clear the flag on Newline/Eof.
            if tok.token == Token::HereDocOp {
                self.expecting_heredoc_delimiter = true;
                self.pending_strip_tabs = false;
            } else if tok.token == Token::HereDocStripOp {
                self.expecting_heredoc_delimiter = true;
                self.pending_strip_tabs = true;
            }
            return Ok(tok);
        }

        // Word or IO_NUMBER
        let tok = self.scan_word(start)?;

        // If we're expecting a heredoc delimiter, register it
        if self.expecting_heredoc_delimiter {
            self.expecting_heredoc_delimiter = false;
            if let Token::Word(ref raw) = tok.token {
                let (delimiter, quoted) = heredoc::strip_heredoc_quotes(raw);
                self.pending_heredocs.push(PendingHereDoc {
                    delimiter,
                    strip_tabs: self.pending_strip_tabs,
                    quoted,
                });
            }
        }

        Ok(tok)
    }

    /// Skip spaces, tabs, and `\<newline>` line continuations (but NOT bare newlines).
    fn skip_blanks(&mut self) {
        loop {
            match self.cursor.peek() {
                Some(' ') | Some('\t') => {
                    self.cursor.advance();
                }
                Some('\\') if self.cursor.peek_second() == Some('\n') => {
                    // Line continuation: \<newline> is removed entirely (POSIX 2.2.1)
                    self.cursor.advance(); // consume backslash
                    self.cursor.advance(); // consume newline
                }
                _ => break,
            }
        }
    }

    /// Skip a comment (`#` to end of line), if present.
    fn skip_comment(&mut self) {
        if self.cursor.peek() == Some('#') {
            while let Some(ch) = self.cursor.peek() {
                if ch == '\n' {
                    break;
                }
                self.cursor.advance();
            }
        }
    }

}

#[cfg(test)]
#[path = "lexer/tests.rs"]
mod tests;
