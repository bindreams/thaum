use crate::error::LexError;
use crate::span::Span;
use crate::token::{ExtGlobTokenKind, GlobKind, SpannedToken, Token};

use super::Lexer;

impl<'src> Lexer<'src> {
    /// Scan one fragment token. Called when the cursor is at a non-blank,
    /// non-operator, non-newline character in normal mode.
    pub(super) fn scan_fragment(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        let ch = self.cursor.peek().unwrap();

        match ch {
            '\'' => self.scan_single_quoted(start),
            '"' => self.scan_double_quoted(start),
            '$' => self.scan_dollar(start),
            '`' => self.scan_backtick(start),
            '\\' => self.scan_backslash_escape(start),
            '~' if !self.word_started => self.scan_tilde_prefix(start),
            '*' | '?' => self.scan_glob_or_extglob(start, ch),
            '[' if self.has_bracket_close_in_word() => self.scan_bracket_glob(start),
            '+' | '@' | '!'
                if self.options.extglob && self.cursor.peek_second() == Some('(') =>
            {
                self.scan_extglob(start, ch)
            }
            '<' | '>'
                if !self.word_started
                    && self.options.process_substitution
                    && self.cursor.peek_second() == Some('(') =>
            {
                self.scan_process_sub(start, ch)
            }
            _ => self.scan_literal(start),
        }
    }

    /// Scan unquoted literal text until a word delimiter, operator character,
    /// or fragment boundary.
    fn scan_literal(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        let mut literal = String::new();
        let mut all_digits = true;

        while let Some(ch) = self.cursor.peek() {
            match ch {
                // Word delimiters
                ' ' | '\t' | '\n' => break,
                // Operator characters
                '|' | '&' | ';' | '(' | ')' => break,
                // < and > break unless mid-word process sub detection already handled them
                '<' | '>' => {
                    // IoNumber detection: all digits followed by < or >
                    if all_digits && !literal.is_empty() {
                        if let Ok(fd) = literal.parse::<i32>() {
                            return Ok(SpannedToken {
                                token: Token::IoNumber(fd),
                                span: Span::new(start, self.cursor.pos().0),
                            });
                        }
                    }
                    break;
                }
                // Fragment boundaries — stop literal, next scan_fragment handles these
                '$' | '\'' | '"' | '\\' | '`' | '*' | '?' => break,
                // [ only breaks if it starts a bracket glob (] found in same word)
                '[' if self.has_bracket_close_in_word() => break,
                // Tilde at word start is a fragment boundary (but only if literal is empty,
                // meaning ~ would be handled by scan_tilde_prefix from scan_fragment)
                '~' if literal.is_empty() && !self.word_started => break,
                // ExtGlob prefix with ( following
                '+' | '@' | '!'
                    if self.options.extglob && self.cursor.peek_second() == Some('(') =>
                {
                    break;
                }
                // Hash at word start is a comment
                '#' if literal.is_empty() && !self.word_started => break,
                _ => {
                    if !ch.is_ascii_digit() {
                        all_digits = false;
                    }
                    literal.push(ch);
                    self.cursor.advance();
                }
            }
        }

        // IoNumber detection at EOF or when followed by < or >
        if all_digits && !literal.is_empty() {
            if let Some(ch) = self.cursor.peek() {
                if ch == '<' || ch == '>' {
                    if let Ok(fd) = literal.parse::<i32>() {
                        return Ok(SpannedToken {
                            token: Token::IoNumber(fd),
                            span: Span::new(start, self.cursor.pos().0),
                        });
                    }
                }
            }
        }

        Ok(SpannedToken {
            token: Token::Literal(literal),
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a single-quoted string: content between '...' (without quote chars).
    fn scan_single_quoted(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        self.cursor.advance(); // consume opening '
        let quote_start = self.cursor.pos().0 - 1;
        let mut content = String::new();
        loop {
            match self.cursor.advance() {
                Some('\'') => break,
                Some(c) => content.push(c),
                None => {
                    return Err(LexError::UnterminatedSingleQuote {
                        span: Span::new(quote_start, self.cursor.pos().0),
                    });
                }
            }
        }
        Ok(SpannedToken {
            token: Token::SingleQuoted(content),
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a double-quoted string: raw content between "..." (without quote chars).
    /// The content is not parsed here — the parser spawns an inner lexer for it.
    fn scan_double_quoted(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        self.cursor.advance(); // consume opening "
        let quote_start = self.cursor.pos().0 - 1;
        let mut content = String::new();
        loop {
            match self.cursor.advance() {
                Some('"') => break,
                Some('\\') => {
                    // Preserve backslash escapes in raw content for inner lexer
                    content.push('\\');
                    if let Some(c) = self.cursor.advance() {
                        content.push(c);
                    }
                }
                // $(...) / $((...)) / ${...} create new quoting contexts
                Some('$') => {
                    content.push('$');
                    match self.cursor.peek() {
                        Some('(') => {
                            self.cursor.advance();
                            content.push('(');
                            if self.cursor.peek() == Some('(') {
                                self.cursor.advance();
                                content.push('(');
                                self.read_balanced_into(
                                    &mut content,
                                    '(',
                                    ')',
                                    2,
                                    quote_start,
                                )?;
                            } else {
                                self.read_balanced_into(
                                    &mut content,
                                    '(',
                                    ')',
                                    1,
                                    quote_start,
                                )?;
                            }
                        }
                        Some('{') => {
                            self.cursor.advance();
                            content.push('{');
                            self.read_balanced_into(
                                &mut content,
                                '{',
                                '}',
                                1,
                                quote_start,
                            )?;
                        }
                        _ => {}
                    }
                }
                // Backtick command substitution creates a new quoting context
                Some('`') => {
                    content.push('`');
                    loop {
                        match self.cursor.advance() {
                            Some('`') => {
                                content.push('`');
                                break;
                            }
                            Some('\\') => {
                                content.push('\\');
                                if let Some(c) = self.cursor.advance() {
                                    content.push(c);
                                }
                            }
                            Some(c) => content.push(c),
                            None => {
                                return Err(LexError::UnterminatedBackquote {
                                    span: Span::new(quote_start, self.cursor.pos().0),
                                });
                            }
                        }
                    }
                }
                Some(c) => content.push(c),
                None => {
                    return Err(LexError::UnterminatedDoubleQuote {
                        span: Span::new(quote_start, self.cursor.pos().0),
                    });
                }
            }
        }
        Ok(SpannedToken {
            token: Token::DoubleQuoted(content),
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a dollar-prefixed construct: $VAR, ${...}, $(...), $((...)),
    /// $'...', $"...", or lone $.
    pub(super) fn scan_dollar(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        self.cursor.advance(); // consume $
        let dollar_pos = self.cursor.pos().0 - 1;

        match self.cursor.peek() {
            Some('(') => {
                self.cursor.advance(); // consume (
                if self.cursor.peek() == Some('(') {
                    // Arithmetic expansion: $((expr))
                    self.cursor.advance(); // consume second (
                    let content = self.read_balanced_content('(', ')', 2, dollar_pos)?;
                    Ok(SpannedToken {
                        token: Token::ArithSub(content),
                        span: Span::new(start, self.cursor.pos().0),
                    })
                } else {
                    // Command substitution: $(cmd)
                    let content = self.read_balanced_content('(', ')', 1, dollar_pos)?;
                    Ok(SpannedToken {
                        token: Token::CommandSub(content),
                        span: Span::new(start, self.cursor.pos().0),
                    })
                }
            }
            Some('{') => {
                self.cursor.advance(); // consume {
                let content = self.read_balanced_content('{', '}', 1, dollar_pos)?;
                Ok(SpannedToken {
                    token: Token::BraceParam(content),
                    span: Span::new(start, self.cursor.pos().0),
                })
            }
            // $'...' — ANSI-C quoting (Bash, only in normal mode)
            Some('\'') if self.mode == super::LexerMode::Normal && self.options.ansi_c_quoting => {
                self.cursor.advance(); // consume '
                let mut content = String::new();
                loop {
                    match self.cursor.advance() {
                        Some('\'') => break,
                        Some('\\') => {
                            // Keep escape sequences literally — the executor interprets them
                            content.push('\\');
                            if let Some(c) = self.cursor.advance() {
                                content.push(c);
                            }
                        }
                        Some(c) => content.push(c),
                        None => {
                            return Err(LexError::UnterminatedSingleQuote {
                                span: Span::new(dollar_pos, self.cursor.pos().0),
                            });
                        }
                    }
                }
                Ok(SpannedToken {
                    token: Token::BashAnsiCQuoted(content),
                    span: Span::new(start, self.cursor.pos().0),
                })
            }
            // $"..." — locale translation (Bash, only in normal mode)
            Some('"') if self.mode == super::LexerMode::Normal && self.options.locale_translation => {
                self.cursor.advance(); // consume "
                let mut content = String::new();
                loop {
                    match self.cursor.advance() {
                        Some('"') => break,
                        Some('\\') => {
                            content.push('\\');
                            if let Some(c) = self.cursor.advance() {
                                content.push(c);
                            }
                        }
                        Some(c) => content.push(c),
                        None => {
                            return Err(LexError::UnterminatedDoubleQuote {
                                span: Span::new(dollar_pos, self.cursor.pos().0),
                            });
                        }
                    }
                }
                Ok(SpannedToken {
                    token: Token::BashLocaleQuoted(content),
                    span: Span::new(start, self.cursor.pos().0),
                })
            }
            Some(c) if c.is_ascii_alphanumeric() || c == '_' || is_special_param(c) => {
                let name = self.scan_param_name();
                Ok(SpannedToken {
                    token: Token::SimpleParam(name),
                    span: Span::new(start, self.cursor.pos().0),
                })
            }
            _ => {
                // Lone $ is literal
                Ok(SpannedToken {
                    token: Token::Literal("$".to_string()),
                    span: Span::new(start, self.cursor.pos().0),
                })
            }
        }
    }

    /// Scan a simple parameter name after $: $name, $1, $@, etc.
    fn scan_param_name(&mut self) -> String {
        let mut name = String::new();
        if let Some(c) = self.cursor.peek() {
            if is_special_param(c) {
                name.push(c);
                self.cursor.advance();
                return name;
            }
            if c.is_ascii_digit() {
                name.push(c);
                self.cursor.advance();
                return name;
            }
        }
        // Regular name: [A-Za-z_][A-Za-z0-9_]*
        while let Some(c) = self.cursor.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                name.push(c);
                self.cursor.advance();
            } else {
                break;
            }
        }
        name
    }

    /// Scan a backtick command substitution: `...`
    /// Check if there's a `]` before the next word delimiter starting from
    /// the current position. Used to decide if `[` starts a bracket glob.
    fn has_bracket_close_in_word(&self) -> bool {
        let source = &self.cursor.source[self.cursor.pos..];
        let mut chars = source.chars();
        chars.next(); // skip the [
        // POSIX bracket expression rules: negation and first ] are special
        if matches!(chars.clone().next(), Some('!') | Some('^')) {
            chars.next();
        }
        if chars.clone().next() == Some(']') {
            chars.next(); // ] immediately after [ or [! is literal
        }
        for c in chars {
            match c {
                ']' => return true,
                ' ' | '\t' | '\n' | '|' | '&' | ';' | '<' | '>' | '(' | ')' => return false,
                _ => continue,
            }
        }
        false
    }

    pub(super) fn scan_backtick(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        self.cursor.advance(); // consume `
        let bt_start = self.cursor.pos().0 - 1;
        let mut content = String::new();
        loop {
            match self.cursor.advance() {
                Some('`') => break,
                Some('\\') => {
                    if let Some(c) = self.cursor.advance() {
                        if c == '`' || c == '\\' || c == '$' {
                            content.push(c);
                        } else {
                            content.push('\\');
                            content.push(c);
                        }
                    }
                }
                Some(c) => content.push(c),
                None => {
                    return Err(LexError::UnterminatedBackquote {
                        span: Span::new(bt_start, self.cursor.pos().0),
                    });
                }
            }
        }
        Ok(SpannedToken {
            token: Token::BacktickSub(content),
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a backslash escape in unquoted context.
    fn scan_backslash_escape(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        if self.cursor.peek_second() == Some('\n') {
            // Line continuation: \<newline> is removed entirely (POSIX 2.2.1)
            self.cursor.advance(); // skip backslash
            self.cursor.advance(); // skip newline
            // Continue to the next fragment (line continuation is invisible)
            // Emit nothing — let next_token call scan_fragment again.
            // But we're in scan_fragment which must return a token...
            // Recursively try the next fragment.
            let new_start = self.cursor.pos().0;
            if self.cursor.is_eof() {
                return Ok(SpannedToken {
                    token: Token::Eof,
                    span: Span::empty(new_start),
                });
            }
            // Check if the next char is still a fragment character
            let ch = self.cursor.peek().unwrap();
            if ch == ' ' || ch == '\t' || ch == '\n' {
                // Word delimiter — return an empty literal? No, we should let
                // next_token handle this. But we're mid-scan_fragment...
                // Actually, the line continuation should have been consumed in
                // scan_blanks_and_comments or scan_literal. Having it reach
                // scan_backslash_escape means the cursor is at \<newline> at
                // the start of a fragment. After consuming it, the next thing
                // could be a blank. We need to return *something*.
                // Return an empty literal and let next_token handle the blank.
                return Ok(SpannedToken {
                    token: Token::Literal(String::new()),
                    span: Span::new(start, new_start),
                });
            }
            return self.scan_fragment(new_start);
        }

        // Regular backslash escape: preserve backslash + escaped char as raw literal
        self.cursor.advance(); // consume backslash
        let mut raw = String::from('\\');
        if let Some(c) = self.cursor.advance() {
            raw.push(c);
        }
        Ok(SpannedToken {
            token: Token::Literal(raw),
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a tilde prefix: ~user at the start of a word.
    fn scan_tilde_prefix(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        self.cursor.advance(); // consume ~
        let mut user = String::new();
        while let Some(ch) = self.cursor.peek() {
            if ch == '/' || ch == ':' || ch == ' ' || ch == '\t' || ch == '\n' {
                break;
            }
            // Stop at any special character that starts a different fragment
            if ch == '$' || ch == '`' || ch == '\'' || ch == '"' || ch == '\\' {
                break;
            }
            // Stop at operator characters
            if ch == '|' || ch == '&' || ch == ';' || ch == '<' || ch == '>' || ch == '(' || ch == ')' {
                break;
            }
            user.push(ch);
            self.cursor.advance();
        }
        Ok(SpannedToken {
            token: Token::TildePrefix(user),
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a glob character (* or ?) with extglob detection.
    fn scan_glob_or_extglob(
        &mut self,
        start: usize,
        ch: char,
    ) -> Result<SpannedToken, LexError> {
        // Check for extglob: ?(...) or *(...)
        if self.options.extglob && self.cursor.peek_second() == Some('(') {
            return self.scan_extglob(start, ch);
        }

        self.cursor.advance(); // consume * or ?
        let kind = match ch {
            '*' => GlobKind::Star,
            '?' => GlobKind::Question,
            _ => unreachable!(),
        };
        Ok(SpannedToken {
            token: Token::Glob(kind),
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a bracket glob expression: [ followed by bracket content through ].
    fn scan_bracket_glob(&mut self, start: usize) -> Result<SpannedToken, LexError> {
        self.cursor.advance(); // consume [
        let glob_span = Span::new(start, self.cursor.pos().0);

        // Read bracket content through closing ]
        let bracket_start = self.cursor.pos().0;
        let mut bracket_content = String::new();

        // Handle negation and first ]
        if self.cursor.peek() == Some('!') || self.cursor.peek() == Some('^') {
            bracket_content.push(self.cursor.advance().unwrap());
        }
        if self.cursor.peek() == Some(']') {
            bracket_content.push(self.cursor.advance().unwrap());
        }
        loop {
            match self.cursor.advance() {
                Some(']') => {
                    bracket_content.push(']');
                    break;
                }
                Some(c) => bracket_content.push(c),
                None => break, // unterminated — let it be handled as literal
            }
        }

        // Queue the bracket content as a Literal token
        if !bracket_content.is_empty() {
            self.queued_tokens.push_back(SpannedToken {
                token: Token::Literal(bracket_content),
                span: Span::new(bracket_start, self.cursor.pos().0),
            });
        }

        Ok(SpannedToken {
            token: Token::Glob(GlobKind::BracketOpen),
            span: glob_span,
        })
    }

    /// Scan an extended glob pattern: ?(pat), *(pat), +(pat), @(pat), !(pat).
    fn scan_extglob(&mut self, start: usize, prefix: char) -> Result<SpannedToken, LexError> {
        let kind = match prefix {
            '?' => ExtGlobTokenKind::ZeroOrOne,
            '*' => ExtGlobTokenKind::ZeroOrMore,
            '+' => ExtGlobTokenKind::OneOrMore,
            '@' => ExtGlobTokenKind::ExactlyOne,
            '!' => ExtGlobTokenKind::Not,
            _ => unreachable!(),
        };
        self.cursor.advance(); // consume prefix char
        self.cursor.advance(); // consume (

        let mut pattern = String::new();
        let mut depth = 1;
        loop {
            match self.cursor.advance() {
                Some('(') => {
                    depth += 1;
                    pattern.push('(');
                }
                Some(')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    pattern.push(')');
                }
                Some(c) => pattern.push(c),
                None => break,
            }
        }
        Ok(SpannedToken {
            token: Token::BashExtGlob { kind, pattern },
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a process substitution: <(cmd) or >(cmd).
    fn scan_process_sub(
        &mut self,
        start: usize,
        direction: char,
    ) -> Result<SpannedToken, LexError> {
        self.cursor.advance(); // consume < or >
        self.cursor.advance(); // consume (
        let content = self.read_balanced_content('(', ')', 1, start)?;
        Ok(SpannedToken {
            token: Token::BashProcessSub {
                direction,
                content,
            },
            span: Span::new(start, self.cursor.pos().0),
        })
    }

    /// Scan a heredoc delimiter as a single raw Literal token.
    /// Uses the same logic as the old scan_word — collects everything including
    /// quotes into one flat string so strip_heredoc_quotes can process it.
    pub(super) fn scan_heredoc_delimiter(
        &mut self,
        start: usize,
    ) -> Result<SpannedToken, LexError> {
        let mut word = String::new();

        while let Some(ch) = self.cursor.peek() {
            match ch {
                ' ' | '\t' | '\n' => break,
                '|' | '&' | ';' | '<' | '>' | '(' | ')' => break,

                '\'' => {
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

                '"' => {
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

                '\\' => {
                    self.cursor.advance();
                    word.push('\\');
                    if let Some(c) = self.cursor.advance() {
                        word.push(c);
                    }
                }

                _ => {
                    word.push(ch);
                    self.cursor.advance();
                }
            }
        }

        let end = self.cursor.pos().0;
        if word.is_empty() {
            return Ok(SpannedToken {
                token: Token::Eof,
                span: Span::empty(start),
            });
        }

        Ok(SpannedToken {
            token: Token::Literal(word),
            span: Span::new(start, end),
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

    /// Read balanced content and return it as a String (without the closing
    /// delimiter characters). Uses read_balanced_into internally.
    fn read_balanced_content(
        &mut self,
        open: char,
        close: char,
        initial_depth: i32,
        start: usize,
    ) -> Result<String, LexError> {
        let mut content = String::new();
        self.read_balanced_into(&mut content, open, close, initial_depth, start)?;
        // Strip the closing delimiter(s) that belong to the opening sequence
        let strip_bytes = initial_depth as usize * close.len_utf8();
        content.truncate(content.len() - strip_bytes);
        Ok(content)
    }
}

/// Returns true if c is a POSIX special parameter character.
fn is_special_param(c: char) -> bool {
    matches!(c, '@' | '*' | '#' | '?' | '-' | '$' | '!' | '0')
}
