use crate::error::LexError;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use super::Lexer;

impl<'src> Lexer<'src> {
    /// Scan a word token. Handles quoting, and classifies as IO_NUMBER
    /// when appropriate. Never promotes words to reserved word tokens.
    pub(super) fn scan_word(&mut self, start: usize) -> Result<SpannedToken, LexError> {
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
                            // $(...) / $((...)) / ${...} create new quoting contexts
                            Some('$') => {
                                word.push('$');
                                match self.cursor.peek() {
                                    Some('(') => {
                                        self.cursor.advance();
                                        word.push('(');
                                        if self.cursor.peek() == Some('(') {
                                            self.cursor.advance();
                                            word.push('(');
                                            self.read_balanced_into(
                                                &mut word, '(', ')', 2, quote_start,
                                            )?;
                                        } else {
                                            self.read_balanced_into(
                                                &mut word, '(', ')', 1, quote_start,
                                            )?;
                                        }
                                    }
                                    Some('{') => {
                                        self.cursor.advance();
                                        word.push('{');
                                        self.read_balanced_into(
                                            &mut word, '{', '}', 1, quote_start,
                                        )?;
                                    }
                                    _ => {}
                                }
                            }
                            // Backtick command substitution creates a new quoting context
                            Some('`') => {
                                word.push('`');
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
                                                span: Span::new(
                                                    quote_start,
                                                    self.cursor.pos().0,
                                                ),
                                            });
                                        }
                                    }
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
                    if self.cursor.peek_second() == Some('\n') {
                        // Line continuation: \<newline> is removed entirely (POSIX 2.2.1)
                        self.cursor.advance(); // skip backslash
                        self.cursor.advance(); // skip newline
                        continue;
                    }
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
    pub(super) fn read_balanced_into(
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
