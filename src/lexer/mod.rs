mod cursor;
pub(crate) mod heredoc;

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

    /// Skip spaces and tabs (but NOT newlines).
    fn skip_blanks(&mut self) {
        while let Some(ch) = self.cursor.peek() {
            if ch == ' ' || ch == '\t' {
                self.cursor.advance();
            } else {
                break;
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

    /// Try to scan a multi-char or single-char operator. Returns `None` if the
    /// current character doesn't start an operator.
    fn try_scan_operator(
        &mut self,
        start: usize,
        preceded_by_blank: bool,
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
                Some('(') if self.options.process_substitution && preceded_by_blank => {
                    return Ok(None);
                }
                _ => (Token::RedirectFromFile, 1),
            },
            '>' => match self.cursor.peek_second() {
                Some('>') => (Token::Append, 2),
                Some('&') => (Token::RedirectToFd, 2),
                Some('|') => (Token::Clobber, 2),
                Some('(') if self.options.process_substitution && preceded_by_blank => {
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

    /// Scan a word token. Handles quoting, and classifies as IO_NUMBER
    /// when appropriate. Never promotes words to reserved word tokens.
    fn scan_word(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        let mut word = String::new();
        let mut all_digits = true;

        while let Some(ch) = self.cursor.peek() {
            match ch {
                // Word delimiters
                ' ' | '\t' | '\n' => break,

                // Process substitution <(...) or >(...) (Bash)
                // Only when word is empty (standalone token). Mid-word `<(` is
                // impossible here because `<` breaks the word at the operator
                // match below, and try_scan_operator only allows `<(` as
                // process substitution when preceded by whitespace.
                '<' | '>'
                    if word.is_empty()
                        && self.options.process_substitution
                        && self.cursor.peek_second() == Some('(') =>
                {
                    all_digits = false;
                    word.push(ch);
                    self.cursor.advance(); // consume < or >
                    word.push('(');
                    self.cursor.advance(); // consume (
                    let proc_start = self.cursor.pos().0 - 2;
                    self.read_balanced_into(&mut word, '(', ')', 1, proc_start)?;
                }

                // Extended globbing: ?(...), *(...), +(...), @(...), !(...)
                // When `(` follows an extglob prefix character, read balanced
                // parens into the word instead of breaking.
                '(' if self.options.extglob
                    && word
                        .as_bytes()
                        .last()
                        .is_some_and(|&c| matches!(c, b'?' | b'*' | b'+' | b'@' | b'!')) =>
                {
                    all_digits = false;
                    word.push('(');
                    self.cursor.advance();
                    let ext_start = self.cursor.pos().0 - 1;
                    self.read_balanced_into(&mut word, '(', ')', 1, ext_start)?;
                }

                '|' | '&' | ';' | '<' | '>' | '(' | ')' => {
                    // Check for IO_NUMBER: all digits followed by < or >
                    if all_digits && !word.is_empty() && (ch == '<' || ch == '>') {
                        if let Ok(fd) = word.parse::<i32>() {
                            return Ok(SpannedToken {
                                token: Token::IoNumber(fd),
                                span: Span::new(start, self.cursor.pos().0),
                            });
                        }
                    }
                    break;
                }

                // Single-quoted string
                '\'' => {
                    all_digits = false;
                    self.cursor.advance();
                    word.push('\'');
                    let quote_start = self.cursor.pos().0 - 1;
                    loop {
                        match self.cursor.advance() {
                            Some('\'') => {
                                word.push('\'');
                                break;
                            }
                            Some(c) => word.push(c),
                            None => {
                                return Err(LexError::UnterminatedSingleQuote {
                                    span: Span::new(quote_start, self.cursor.pos().0),
                                });
                            }
                        }
                    }
                }

                // Double-quoted string
                '"' => {
                    all_digits = false;
                    self.cursor.advance();
                    word.push('"');
                    let quote_start = self.cursor.pos().0 - 1;
                    loop {
                        match self.cursor.advance() {
                            Some('"') => {
                                word.push('"');
                                break;
                            }
                            Some('\\') => {
                                word.push('\\');
                                if let Some(c) = self.cursor.advance() {
                                    word.push(c);
                                }
                            }
                            Some(c) => word.push(c),
                            None => {
                                return Err(LexError::UnterminatedDoubleQuote {
                                    span: Span::new(quote_start, self.cursor.pos().0),
                                });
                            }
                        }
                    }
                }

                // Backslash escape (outside quotes)
                '\\' => {
                    all_digits = false;
                    self.cursor.advance();
                    word.push('\\');
                    if let Some(c) = self.cursor.advance() {
                        word.push(c);
                    }
                }

                // Backtick (command substitution)
                '`' => {
                    all_digits = false;
                    self.cursor.advance();
                    word.push('`');
                    let bt_start = self.cursor.pos().0 - 1;
                    loop {
                        match self.cursor.advance() {
                            Some('`') => {
                                word.push('`');
                                break;
                            }
                            Some('\\') => {
                                word.push('\\');
                                if let Some(c) = self.cursor.advance() {
                                    word.push(c);
                                }
                            }
                            Some(c) => word.push(c),
                            None => {
                                return Err(LexError::UnterminatedBackquote {
                                    span: Span::new(bt_start, self.cursor.pos().0),
                                });
                            }
                        }
                    }
                }

                // Dollar sign followed by ( or { — read balanced content
                '$' => {
                    all_digits = false;
                    word.push('$');
                    self.cursor.advance();

                    let dollar_pos = self.cursor.pos().0 - 1;
                    match self.cursor.peek() {
                        Some('(') => {
                            self.cursor.advance();
                            word.push('(');
                            if self.cursor.peek() == Some('(') {
                                // $(( — arithmetic expansion
                                self.cursor.advance();
                                word.push('(');
                                self.read_balanced_into(&mut word, '(', ')', 2, dollar_pos)?;
                            } else {
                                // $( — command substitution
                                self.read_balanced_into(&mut word, '(', ')', 1, dollar_pos)?;
                            }
                        }
                        Some('{') => {
                            self.cursor.advance();
                            word.push('{');
                            self.read_balanced_into(&mut word, '{', '}', 1, dollar_pos)?;
                        }
                        // $'...' — ANSI-C quoting (Bash)
                        Some('\'') if self.options.ansi_c_quoting => {
                            self.cursor.advance();
                            word.push('\'');
                            loop {
                                match self.cursor.advance() {
                                    Some('\'') => {
                                        word.push('\'');
                                        break;
                                    }
                                    Some('\\') => {
                                        word.push('\\');
                                        if let Some(c) = self.cursor.advance() {
                                            word.push(c);
                                        }
                                    }
                                    Some(c) => word.push(c),
                                    None => {
                                        return Err(LexError::UnterminatedSingleQuote {
                                            span: Span::new(dollar_pos, self.cursor.pos().0),
                                        });
                                    }
                                }
                            }
                        }
                        // $"..." — locale translation (Bash)
                        Some('"') if self.options.locale_translation => {
                            self.cursor.advance();
                            word.push('"');
                            loop {
                                match self.cursor.advance() {
                                    Some('"') => {
                                        word.push('"');
                                        break;
                                    }
                                    Some('\\') => {
                                        word.push('\\');
                                        if let Some(c) = self.cursor.advance() {
                                            word.push(c);
                                        }
                                    }
                                    Some(c) => word.push(c),
                                    None => {
                                        return Err(LexError::UnterminatedDoubleQuote {
                                            span: Span::new(dollar_pos, self.cursor.pos().0),
                                        });
                                    }
                                }
                            }
                        }
                        _ => {
                            // $name, $?, etc. — just continue normal word scanning
                        }
                    }
                }

                // Hash at the start of a word is a comment
                '#' if word.is_empty() => break,

                // Regular character
                _ => {
                    if !ch.is_ascii_digit() {
                        all_digits = false;
                    }
                    word.push(ch);
                    self.cursor.advance();
                }
            }
        }

        let end = self.cursor.pos().0;
        let span = Span::new(start, end);

        if word.is_empty() {
            // Shouldn't get here if the caller checked for EOF, but just in case
            return Ok(SpannedToken {
                token: Token::Eof,
                span: Span::empty(start),
            });
        }

        Ok(SpannedToken {
            token: Token::Word(word),
            span,
        })
    }

    /// Read characters into `word` until matching close delimiter is found.
    /// Handles nested open/close pairs and quoting within the balanced content.
    /// Returns an error if EOF is reached without finding the closing delimiter.
    fn read_balanced_into(
        &mut self,
        word: &mut String,
        open: char,
        close: char,
        mut depth: i32,
        start: usize,
    ) -> Result<(), LexError> {
        while let Some(ch) = self.cursor.advance() {
            word.push(ch);
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            } else if ch == '\'' {
                loop {
                    match self.cursor.advance() {
                        Some('\'') => {
                            word.push('\'');
                            break;
                        }
                        Some(c) => word.push(c),
                        None => {
                            return Err(LexError::UnterminatedSingleQuote {
                                span: Span::new(start, self.cursor.pos().0),
                            });
                        }
                    }
                }
            } else if ch == '"' {
                loop {
                    match self.cursor.advance() {
                        Some('"') => {
                            word.push('"');
                            break;
                        }
                        Some('\\') => {
                            word.push('\\');
                            if let Some(c) = self.cursor.advance() {
                                word.push(c);
                            }
                        }
                        Some(c) => word.push(c),
                        None => {
                            return Err(LexError::UnterminatedDoubleQuote {
                                span: Span::new(start, self.cursor.pos().0),
                            });
                        }
                    }
                }
            } else if ch == '\\' {
                if let Some(c) = self.cursor.advance() {
                    word.push(c);
                }
            } else if ch == '`' {
                loop {
                    match self.cursor.advance() {
                        Some('`') => {
                            word.push('`');
                            break;
                        }
                        Some('\\') => {
                            word.push('\\');
                            if let Some(c) = self.cursor.advance() {
                                word.push(c);
                            }
                        }
                        Some(c) => word.push(c),
                        None => {
                            return Err(LexError::UnterminatedBackquote {
                                span: Span::new(start, self.cursor.pos().0),
                            });
                        }
                    }
                }
            }
        }
        // Reached EOF without finding matching close delimiter
        let kind = match (open, close) {
            ('(', ')') => "command substitution — missing ')'".to_string(),
            ('{', '}') => "parameter expansion — missing '}'".to_string(),
            _ => format!("expression — missing '{}'", close),
        };
        Err(LexError::UnterminatedExpansion {
            kind,
            span: Span::new(start, self.cursor.pos().0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: lex all tokens from input.
    fn lex_all(input: &str) -> Result<Vec<Token>, LexError> {
        let mut lexer = Lexer::new(input, ParseOptions::default());
        let mut tokens = Vec::new();
        loop {
            let st = lexer.next_token()?;
            if st.token == Token::Eof {
                break;
            }
            tokens.push(st.token);
        }
        Ok(tokens)
    }

    // === Empty / EOF ===

    #[test]
    fn lex_empty_input() {
        let tokens = lex_all("").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn lex_only_whitespace() {
        let tokens = lex_all("   \t  ").unwrap();
        assert!(tokens.is_empty());
    }

    // === Words ===

    #[test]
    fn lex_single_word() {
        let tokens = lex_all("hello").unwrap();
        assert_eq!(tokens, vec![Token::Word("hello".into())]);
    }

    #[test]
    fn lex_multiple_words() {
        let tokens = lex_all("echo hello world").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Word("echo".into()),
                Token::Word("hello".into()),
                Token::Word("world".into()),
            ]
        );
    }

    #[test]
    fn lex_word_with_numbers() {
        let tokens = lex_all("file123").unwrap();
        assert_eq!(tokens, vec![Token::Word("file123".into())]);
    }

    // === Newlines ===

    #[test]
    fn lex_newline() {
        let tokens = lex_all("a\nb").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Word("a".into()),
                Token::Newline,
                Token::Word("b".into()),
            ]
        );
    }

    // === Single-character operators ===

    #[test]
    fn lex_single_char_operators() {
        let tokens = lex_all("| ; & < > ( )").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Pipe,
                Token::Semicolon,
                Token::Ampersand,
                Token::RedirectFromFile,
                Token::RedirectToFile,
                Token::LParen,
                Token::RParen,
            ]
        );
    }

    // === Multi-character operators ===

    #[test]
    fn lex_multi_char_operators() {
        let tokens = lex_all("&& || ;; << >> <& >& <> >|").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::AndIf,
                Token::OrIf,
                Token::CaseBreak,
                Token::HereDocOp,
                Token::Append,
                Token::RedirectFromFd,
                Token::RedirectToFd,
                Token::ReadWrite,
                Token::Clobber,
            ]
        );
    }

    #[test]
    fn lex_dlessdash() {
        let tokens = lex_all("<<-").unwrap();
        assert_eq!(tokens, vec![Token::HereDocStripOp]);
    }

    #[test]
    fn lex_operator_longest_prefix() {
        // `<<` should not be split into `<` `<`
        let tokens = lex_all("<<EOF").unwrap();
        assert_eq!(tokens, vec![Token::HereDocOp, Token::Word("EOF".into())]);
    }

    #[test]
    fn lex_operator_disambiguation() {
        // `>|` is Clobber, not Great + Pipe
        let tokens = lex_all(">|").unwrap();
        assert_eq!(tokens, vec![Token::Clobber]);
    }

    // === IO_NUMBER ===

    #[test]
    fn lex_io_number_before_great() {
        let tokens = lex_all("2>").unwrap();
        assert_eq!(tokens, vec![Token::IoNumber(2), Token::RedirectToFile]);
    }

    #[test]
    fn lex_io_number_before_less() {
        let tokens = lex_all("0<").unwrap();
        assert_eq!(tokens, vec![Token::IoNumber(0), Token::RedirectFromFile]);
    }

    #[test]
    fn lex_number_with_space_is_word() {
        // Space between number and operator → just a word
        let tokens = lex_all("2 >").unwrap();
        assert_eq!(tokens, vec![Token::Word("2".into()), Token::RedirectToFile]);
    }

    #[test]
    fn lex_non_number_before_redirect_is_word() {
        let tokens = lex_all("abc>").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Word("abc".into()), Token::RedirectToFile]
        );
    }

    // === Comments ===

    #[test]
    fn lex_comment_skipped() {
        let tokens = lex_all("# this is a comment").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn lex_comment_after_word() {
        let tokens = lex_all("echo hello # comment").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Word("echo".into()), Token::Word("hello".into())]
        );
    }

    #[test]
    fn lex_hash_inside_word_not_comment() {
        let tokens = lex_all("foo#bar").unwrap();
        assert_eq!(tokens, vec![Token::Word("foo#bar".into())]);
    }

    // === Quoting ===

    #[test]
    fn lex_single_quoted_word() {
        let tokens = lex_all("'hello world'").unwrap();
        assert_eq!(tokens, vec![Token::Word("'hello world'".into())]);
    }

    #[test]
    fn lex_double_quoted_word() {
        let tokens = lex_all("\"hello world\"").unwrap();
        assert_eq!(tokens, vec![Token::Word("\"hello world\"".into())]);
    }

    #[test]
    fn lex_backslash_escape() {
        let tokens = lex_all("hello\\ world").unwrap();
        assert_eq!(tokens, vec![Token::Word("hello\\ world".into())]);
    }

    #[test]
    fn lex_mixed_quoting() {
        let tokens = lex_all("he'llo '\"wor\"ld").unwrap();
        assert_eq!(tokens, vec![Token::Word("he'llo '\"wor\"ld".into())]);
    }

    #[test]
    fn lex_unterminated_single_quote() {
        let result = lex_all("'hello");
        assert!(matches!(
            result,
            Err(LexError::UnterminatedSingleQuote { .. })
        ));
    }

    #[test]
    fn lex_unterminated_double_quote() {
        let result = lex_all("\"hello");
        assert!(matches!(
            result,
            Err(LexError::UnterminatedDoubleQuote { .. })
        ));
    }

    #[test]
    fn lex_backtick_command_substitution() {
        let tokens = lex_all("`echo hi`").unwrap();
        assert_eq!(tokens, vec![Token::Word("`echo hi`".into())]);
    }

    #[test]
    fn lex_unterminated_backtick() {
        let result = lex_all("`echo hi");
        assert!(matches!(
            result,
            Err(LexError::UnterminatedBackquote { .. })
        ));
    }

    // === Reserved words are NOT promoted by the lexer ===

    #[test]
    fn lex_reserved_words_are_just_words() {
        // The lexer never promotes reserved words — that's the parser's job
        let tokens = lex_all("if then else fi").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Word("if".into()),
                Token::Word("then".into()),
                Token::Word("else".into()),
                Token::Word("fi".into()),
            ]
        );
    }

    #[test]
    fn lex_braces_are_just_words() {
        let tokens = lex_all("{ }").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Word("{".into()), Token::Word("}".into())]
        );
    }

    #[test]
    fn lex_bang_is_just_a_word() {
        let tokens = lex_all("!").unwrap();
        assert_eq!(tokens, vec![Token::Word("!".into())]);
    }

    // === Spans ===

    #[test]
    fn lex_span_tracking() {
        let mut lexer = Lexer::new("echo hello", ParseOptions::default());
        let t1 = lexer.next_token().unwrap();
        assert_eq!(t1.span, Span::new(0, 4));
        assert_eq!(t1.token, Token::Word("echo".into()));

        let t2 = lexer.next_token().unwrap();
        assert_eq!(t2.span, Span::new(5, 10));
        assert_eq!(t2.token, Token::Word("hello".into()));
    }

    #[test]
    fn lex_span_operators() {
        let mut lexer = Lexer::new("&&||", ParseOptions::default());
        let t1 = lexer.next_token().unwrap();
        assert_eq!(t1.span, Span::new(0, 2));

        let t2 = lexer.next_token().unwrap();
        assert_eq!(t2.span, Span::new(2, 4));
    }

    // === Here-documents ===

    #[test]
    fn lex_heredoc_basic() {
        let input = "cat <<EOF\nhello world\nEOF\n";
        let mut lexer = Lexer::new(input, ParseOptions::default());

        assert_eq!(lexer.next_token().unwrap().token, Token::Word("cat".into()));
        assert_eq!(lexer.next_token().unwrap().token, Token::HereDocOp);
        assert_eq!(lexer.next_token().unwrap().token, Token::Word("EOF".into()));

        // Newline triggers heredoc body reading
        assert_eq!(lexer.next_token().unwrap().token, Token::Newline);

        // Body appears as a HereDocBody token after the newline
        let t = lexer.next_token().unwrap();
        if let Token::HereDocBody(body) = &t.token {
            assert_eq!(body, "hello world\n");
        } else {
            panic!("expected HereDocBody, got {:?}", t.token);
        }
    }

    #[test]
    fn lex_heredoc_strip_tabs() {
        let input = "cat <<-EOF\n\thello\n\tworld\n\tEOF\n";
        let mut lexer = Lexer::new(input, ParseOptions::default());

        lexer.next_token().unwrap(); // cat
        lexer.next_token().unwrap(); // <<-
        lexer.next_token().unwrap(); // EOF
        lexer.next_token().unwrap(); // \n

        let t = lexer.next_token().unwrap();
        if let Token::HereDocBody(body) = &t.token {
            assert_eq!(body, "hello\nworld\n");
        } else {
            panic!("expected HereDocBody, got {:?}", t.token);
        }
    }

    #[test]
    fn lex_heredoc_unterminated() {
        let input = "cat <<EOF\nhello world\n";
        let mut lexer = Lexer::new(input, ParseOptions::default());

        lexer.next_token().unwrap(); // cat
        lexer.next_token().unwrap(); // <<
        lexer.next_token().unwrap(); // EOF

        // Newline triggers heredoc read, which fails because EOF delimiter is never found
        let result = lexer.next_token();
        assert!(matches!(result, Err(LexError::UnterminatedHereDoc { .. })));
    }
}
