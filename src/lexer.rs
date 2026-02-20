pub(crate) mod char_source;
pub(crate) mod heredoc;
mod operators;
mod word_scan;

use std::collections::VecDeque;
use std::io::Read;

use crate::dialect::ParseOptions;
use crate::error::{LexError, ParseError};
use crate::span::{BytePos, Span};
use crate::token::{SpannedToken, Token};

use char_source::CharSource;
use heredoc::PendingHereDoc;

/// Lexer operating mode.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexerMode {
    /// Normal shell tokenization: operators, blanks, newlines, fragments.
    Normal,
    /// Inside double-quoted string: only expansions and literal text.
    /// No operators, no word splitting, limited backslash escaping.
    DoubleQuote,
}

/// The shell lexer.
///
/// Emits fragment-level tokens (Literal, SimpleParam, DoubleQuoted, etc.)
/// plus Whitespace tokens as word boundary markers.
///
/// Also provides a buffered token stream with `peek()`/`advance()`/`speculate()`
/// for use by the parser. Call `skip_whitespace()` to consume `Whitespace` tokens before
/// peeking/advancing. Word collection code deliberately skips the call to see
/// `Whitespace` tokens as word boundaries.
pub struct Lexer {
    // --- Constants (never change after construction) ---
    pub(crate) options: ParseOptions,
    mode: LexerMode,

    // --- Character source (forward-only) ---
    pub(super) chars: CharSource,

    // --- Scanning state (only matters at cursor, not touched by speculation) ---
    pending_heredocs: Vec<PendingHereDoc>,
    expecting_heredoc_delimiter: bool,
    pending_strip_tabs: bool,
    /// Whether the last scanned token was Whitespace. Used by operator scanner
    /// for process substitution detection (<( and >( preceded by blank).
    last_was_whitespace: bool,
    /// Whether we've already scanned a fragment for the current word.
    /// Used for tilde prefix detection (~ is special only at word start).
    word_started: bool,

    // --- Stream infrastructure ---
    buffer: VecDeque<SpannedToken>,
    buf_pos: usize,
    speculation_depth: usize,
}

impl Lexer {
    /// Create a lexer from a string source.
    pub fn from_str(source: &str, options: ParseOptions) -> Self {
        Self::build(CharSource::from_str(source), options, LexerMode::Normal)
    }

    /// Create a lexer from any Read source.
    pub fn from_reader(reader: impl Read + 'static, options: ParseOptions) -> Self {
        Self::build(CharSource::from_reader(reader), options, LexerMode::Normal)
    }

    /// Create a lexer in double-quote mode for parsing the inner content
    /// of a double-quoted string.
    pub(crate) fn new_double_quote_mode(source: &str, options: ParseOptions) -> Self {
        Self::build(CharSource::from_str(source), options, LexerMode::DoubleQuote)
    }

    fn build(chars: CharSource, options: ParseOptions, mode: LexerMode) -> Self {
        Lexer {
            options,
            mode,
            chars,
            pending_heredocs: Vec::new(),
            expecting_heredoc_delimiter: false,
            pending_strip_tabs: false,
            last_was_whitespace: false,
            word_started: false,
            buffer: VecDeque::new(),
            buf_pos: 0,
            speculation_depth: 0,
        }
    }

    // ================================================================
    // Character-level helpers (delegate to CharSource)
    // ================================================================

    pub(super) fn peek_char(&self) -> Option<char> {
        self.chars.peek()
    }

    pub(super) fn peek_second_char(&self) -> Option<char> {
        self.chars.peek_at(1)
    }

    pub(super) fn advance_char(&mut self) -> Option<char> {
        self.chars.advance()
    }

    pub(super) fn cursor_pos(&self) -> BytePos {
        BytePos(self.chars.byte_pos())
    }

    pub(super) fn is_at_eof(&self) -> bool {
        self.chars.is_eof()
    }

    // ================================================================
    // Token-level buffered API (merged from TokenStream)
    // ================================================================

    /// Consume all `Whitespace` tokens at the current position.
    pub(crate) fn skip_whitespace(&mut self) -> Result<(), ParseError> {
        loop {
            self.ensure_buffered()?;
            if self.buffer[self.buf_pos].token == Token::Whitespace {
                if self.speculation_depth == 0 {
                    self.buffer.pop_front();
                } else {
                    self.buf_pos += 1;
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Look at the next token without consuming it.
    pub(crate) fn peek(&mut self) -> Result<&SpannedToken, ParseError> {
        self.ensure_buffered()?;
        Ok(&self.buffer[self.buf_pos])
    }

    /// Consume and return the next token.
    pub(crate) fn advance(&mut self) -> Result<SpannedToken, ParseError> {
        self.ensure_buffered()?;
        if self.speculation_depth == 0 {
            debug_assert_eq!(self.buf_pos, 0, "buf_pos should be 0 outside speculation");
            Ok(self.buffer.pop_front().unwrap())
        } else {
            let tok = self.buffer[self.buf_pos].clone();
            self.buf_pos += 1;
            Ok(tok)
        }
    }

    /// Peek at a token at an offset from the current position.
    pub(crate) fn peek_at_offset(
        &mut self,
        offset: usize,
    ) -> Result<&SpannedToken, ParseError> {
        let target = self.buf_pos + offset;
        self.ensure_buffered_at(target)?;
        Ok(&self.buffer[target])
    }

    /// Try a speculative parse. Saves the buffer read position, runs the
    /// closure, and rewinds `buf_pos` if the closure returns `None`.
    /// Tokens scanned during speculation stay in the buffer — scanning state
    /// is purely cursor-side and doesn't need saving.
    pub(crate) fn speculate<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<Option<T>, ParseError>,
    ) -> Result<Option<T>, ParseError> {
        let saved_buf_pos = self.buf_pos;
        self.speculation_depth += 1;
        let result = f(self);
        self.speculation_depth -= 1;
        match result? {
            Some(v) => {
                // Commit: compact consumed tokens from the front
                for _ in 0..self.buf_pos {
                    self.buffer.pop_front();
                }
                self.buf_pos = 0;
                Ok(Some(v))
            }
            None => {
                // Rewind: move read head back, tokens stay in buffer
                self.buf_pos = saved_buf_pos;
                Ok(None)
            }
        }
    }

    fn ensure_buffered(&mut self) -> Result<(), ParseError> {
        while self.buf_pos >= self.buffer.len() {
            self.scan_next()?;
        }
        Ok(())
    }

    fn ensure_buffered_at(&mut self, target: usize) -> Result<(), ParseError> {
        while target >= self.buffer.len() {
            self.scan_next()?;
        }
        Ok(())
    }

    // ================================================================
    // Raw token scanning
    // ================================================================

    /// Get the next token from the source. Scans into the buffer, then
    /// returns the next buffered token.
    ///
    /// Used by external callers (cli, tests, inner double-quote lexers).
    /// The parser uses `peek()`/`advance()` instead, which call `scan_next()`
    /// via `ensure_buffered()`.
    pub(crate) fn next_token(&mut self) -> Result<SpannedToken, LexError> {
        self.scan_next()?;
        if self.speculation_depth == 0 {
            Ok(self.buffer.pop_front().unwrap())
        } else {
            let tok = self.buffer[self.buf_pos].clone();
            self.buf_pos += 1;
            Ok(tok)
        }
    }

    /// Scan the next source construct and push one or more tokens into the buffer.
    ///
    /// The lexer is context-free — it never promotes words to reserved word
    /// tokens. That's the parser's job. The lexer produces fragment tokens
    /// (Literal, SimpleParam, etc.), Whitespace, IoNumber, operators, Newline,
    /// HereDocBody, and Eof.
    fn scan_next(&mut self) -> Result<(), LexError> {
        if self.mode == LexerMode::DoubleQuote {
            let tok = self.next_dq_token()?;
            self.buffer.push_back(tok);
            return Ok(());
        }

        let start = self.cursor_pos().0;

        // Whitespaces + comments -> Whitespace token
        if self.scan_whitespace_and_comments() {
            let end = self.cursor_pos().0;
            self.last_was_whitespace = true;
            self.word_started = false;
            self.buffer.push_back(SpannedToken {
                token: Token::Whitespace,
                span: Span::new(start, end),
            });
            return Ok(());
        }

        // EOF
        if self.is_at_eof() {
            self.buffer.push_back(SpannedToken {
                token: Token::Eof,
                span: Span::empty(start),
            });
            return Ok(());
        }

        let ch = self.peek_char().unwrap();

        // Newline — also triggers heredoc body reading
        if ch == '\n' {
            self.advance_char();
            let newline_span = Span::new(start, start + 1);
            self.last_was_whitespace = false;
            self.word_started = false;

            self.buffer.push_back(SpannedToken {
                token: Token::Newline,
                span: newline_span,
            });

            if !self.pending_heredocs.is_empty() {
                // Read all pending heredoc bodies directly into buffer
                let pending = std::mem::take(&mut self.pending_heredocs);
                for heredoc in &pending {
                    let body =
                        self.read_single_heredoc(&heredoc.delimiter, heredoc.strip_tabs)?;
                    // TODO: The span should cover the actual body content in the source,
                    // not the newline. Track the cursor position before/after reading
                    // the body and use that for the span.
                    self.buffer.push_back(SpannedToken {
                        token: Token::HereDocBody(body),
                        span: newline_span, // wrong — should be the body's span
                    });
                }
            }

            return Ok(());
        }

        // Operators
        if let Some(tok) = self.try_scan_operator(start)? {
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
            self.last_was_whitespace = false;
            self.word_started = false;
            self.buffer.push_back(tok);
            return Ok(());
        }

        // Heredoc delimiter special case: read the entire delimiter as a single
        // Literal token using the old word-scanning logic, so that
        // strip_heredoc_quotes can process it.
        if self.expecting_heredoc_delimiter {
            self.expecting_heredoc_delimiter = false;
            let tok = self.scan_heredoc_delimiter(start)?;
            if let Token::Literal(ref raw) = tok.token {
                let (delimiter, _quoted) = heredoc::strip_heredoc_quotes(raw);
                self.pending_heredocs.push(PendingHereDoc {
                    delimiter,
                    strip_tabs: self.pending_strip_tabs,
                });
            }
            self.last_was_whitespace = false;
            self.word_started = true;
            self.buffer.push_back(tok);
            return Ok(());
        }

        // Fragment token
        let tok = self.scan_fragment(start)?;
        self.last_was_whitespace = false;
        self.word_started = true;
        self.buffer.push_back(tok);
        Ok(())
    }

    /// Tokenize inside a double-quoted string. Only recognizes expansions
    /// ($, backtick) and limited backslash escaping. No operators or word splitting.
    fn next_dq_token(&mut self) -> Result<SpannedToken, LexError> {
        if self.is_at_eof() {
            return Ok(SpannedToken {
                token: Token::Eof,
                span: Span::empty(self.cursor_pos().0),
            });
        }

        let start = self.cursor_pos().0;
        let ch = self.peek_char().unwrap();

        match ch {
            '$' => self.scan_dollar(start),
            '`' => self.scan_backtick(start),
            '\\' => self.scan_dq_backslash(start),
            _ => self.scan_dq_literal(start),
        }
    }

    /// Scan a backslash escape inside double quotes. Only $, `, ", \, and
    /// newline are escapable; other backslashes are literal.
    fn scan_dq_backslash(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        self.advance_char(); // consume backslash
        match self.peek_char() {
            Some(c) if c == '$' || c == '`' || c == '"' || c == '\\' || c == '\n' => {
                self.advance_char();
                Ok(SpannedToken {
                    token: Token::Literal(c.to_string()),
                    span: Span::new(start, self.cursor_pos().0),
                })
            }
            _ => {
                // Backslash is literal
                Ok(SpannedToken {
                    token: Token::Literal("\\".to_string()),
                    span: Span::new(start, self.cursor_pos().0),
                })
            }
        }
    }

    /// Scan literal text inside double quotes until an expansion or EOF.
    fn scan_dq_literal(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        let mut literal = String::new();
        while let Some(ch) = self.peek_char() {
            if ch == '$' || ch == '`' || ch == '\\' {
                break;
            }
            literal.push(ch);
            self.advance_char();
        }
        Ok(SpannedToken {
            token: Token::Literal(literal),
            span: Span::new(start, self.cursor_pos().0),
        })
    }

    /// Skip spaces, tabs, `\<newline>` line continuations, and comments.
    /// Returns true if actual whitespace (spaces/tabs) or a comment was consumed.
    /// Line continuations (`\<newline>`) are consumed but do NOT count as whitespace —
    /// they are invisible joins, not word boundaries.
    fn scan_whitespace_and_comments(&mut self) -> bool {
        let mut has_actual_blank = false;
        loop {
            match self.peek_char() {
                Some(' ') | Some('\t') => {
                    has_actual_blank = true;
                    self.advance_char();
                }
                Some('\\') if self.peek_second_char() == Some('\n') => {
                    // Line continuation: \<newline> is removed entirely (POSIX 2.2.1)
                    self.advance_char(); // consume backslash
                    self.advance_char(); // consume newline
                }
                _ => break,
            }
        }
        // Skip comment if present
        if self.peek_char() == Some('#') {
            has_actual_blank = true;
            while let Some(ch) = self.peek_char() {
                if ch == '\n' {
                    break;
                }
                self.advance_char();
            }
        }
        has_actual_blank
    }
}

#[cfg(test)]
#[path = "lexer/tests.rs"]
mod tests;
