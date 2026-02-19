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
/// plus Blank tokens as word boundary markers.
pub struct Lexer<'src> {
    cursor: Cursor<'src>,
    pending_heredocs: Vec<PendingHereDoc>,
    /// When true, the next token sequence is a heredoc delimiter (scanned as raw Literal).
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
    pub(crate) options: ParseOptions,
    mode: LexerMode,
    /// Whether the last emitted token was Blank. Used by operator scanner
    /// for process substitution detection (<( and >( preceded by blank).
    last_was_blank: bool,
    /// Whether we've already emitted a fragment for the current word.
    /// Used for tilde prefix detection (~ is special only at word start).
    word_started: bool,
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
            mode: LexerMode::Normal,
            last_was_blank: false,
            word_started: false,
        }
    }

    /// Create a lexer in double-quote mode for parsing the inner content
    /// of a double-quoted string.
    pub(crate) fn new_double_quote_mode(source: &'src str, options: ParseOptions) -> Self {
        Lexer {
            cursor: Cursor::new(source),
            pending_heredocs: Vec::new(),
            expecting_heredoc_delimiter: false,
            pending_strip_tabs: false,
            queued_tokens: VecDeque::new(),
            options,
            mode: LexerMode::DoubleQuote,
            last_was_blank: false,
            word_started: false,
        }
    }

    /// Get the next token from the source.
    ///
    /// The lexer is context-free — it never promotes words to reserved word
    /// tokens. That's the parser's job. The lexer produces fragment tokens
    /// (Literal, SimpleParam, etc.), Blank, IoNumber, operators, Newline,
    /// HereDocBody, and Eof.
    pub fn next_token(&mut self) -> Result<SpannedToken, LexError> {
        if self.mode == LexerMode::DoubleQuote {
            return self.next_dq_token();
        }

        // Return queued tokens first (heredoc bodies + deferred newline)
        if let Some(tok) = self.queued_tokens.pop_front() {
            return Ok(tok);
        }

        let start = self.cursor.pos().0;

        // Blanks + comments -> Blank token
        if self.scan_blanks_and_comments() {
            let end = self.cursor.pos().0;
            self.last_was_blank = true;
            self.word_started = false;
            return Ok(SpannedToken {
                token: Token::Blank,
                span: Span::new(start, end),
            });
        }

        // EOF
        if self.cursor.is_eof() {
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
            self.last_was_blank = false;
            self.word_started = false;

            if !self.pending_heredocs.is_empty() {
                // Read all pending heredoc bodies
                let pending = std::mem::take(&mut self.pending_heredocs);
                for heredoc in &pending {
                    let body =
                        self.read_single_heredoc(&heredoc.delimiter, heredoc.strip_tabs)?;
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
            self.last_was_blank = false;
            self.word_started = false;
            return Ok(tok);
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
            self.last_was_blank = false;
            self.word_started = true;
            return Ok(tok);
        }

        // Fragment token
        let tok = self.scan_fragment(start)?;
        self.last_was_blank = false;
        self.word_started = true;
        Ok(tok)
    }

    /// Tokenize inside a double-quoted string. Only recognizes expansions
    /// ($, backtick) and limited backslash escaping. No operators or word splitting.
    fn next_dq_token(&mut self) -> Result<SpannedToken, LexError> {
        if self.cursor.is_eof() {
            return Ok(SpannedToken {
                token: Token::Eof,
                span: Span::empty(self.cursor.pos().0),
            });
        }

        let start = self.cursor.pos().0;
        let ch = self.cursor.peek().unwrap();

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
        self.cursor.advance(); // consume backslash
        match self.cursor.peek() {
            Some(c) if c == '$' || c == '`' || c == '"' || c == '\\' || c == '\n' => {
                self.cursor.advance();
                Ok(SpannedToken {
                    token: Token::Literal(c.to_string()),
                    span: Span::new(start, self.cursor.pos().0),
                })
            }
            _ => {
                // Backslash is literal
                Ok(SpannedToken {
                    token: Token::Literal("\\".to_string()),
                    span: Span::new(start, self.cursor.pos().0),
                })
            }
        }
    }

    /// Scan literal text inside double quotes until an expansion or EOF.
    fn scan_dq_literal(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        let mut literal = String::new();
        while let Some(ch) = self.cursor.peek() {
            if ch == '$' || ch == '`' || ch == '\\' {
                break;
            }
            literal.push(ch);
            self.cursor.advance();
        }
        Ok(SpannedToken {
            token: Token::Literal(literal),
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Skip spaces, tabs, `\<newline>` line continuations, and comments.
    /// Returns true if actual whitespace (spaces/tabs) or a comment was consumed.
    /// Line continuations (`\<newline>`) are consumed but do NOT count as whitespace —
    /// they are invisible joins, not word boundaries.
    fn scan_blanks_and_comments(&mut self) -> bool {
        let mut has_actual_blank = false;
        loop {
            match self.cursor.peek() {
                Some(' ') | Some('\t') => {
                    has_actual_blank = true;
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
        // Skip comment if present
        if self.cursor.peek() == Some('#') {
            has_actual_blank = true;
            while let Some(ch) = self.cursor.peek() {
                if ch == '\n' {
                    break;
                }
                self.cursor.advance();
            }
        }
        has_actual_blank
    }
}

#[cfg(test)]
#[path = "lexer/tests.rs"]
mod tests;
