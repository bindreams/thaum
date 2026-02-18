//! Arithmetic expression tokenizer.
//!
//! A mini-lexer that operates on a raw string (already extracted by the
//! compound parser or word parser). Produces `ArithToken` values consumed
//! by the arithmetic parser in `mod.rs`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArithToken {
    Number(i64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,     // **
    ShiftLeft,    // <<
    ShiftRight,   // >>
    Amp,          // &
    Pipe,         // |
    Caret,        // ^
    AmpAmp,       // &&
    PipePipe,     // ||
    Bang,         // !
    Tilde,        // ~
    EqEq,         // ==
    BangEq,       // !=
    Lt,           // <
    Le,           // <=
    Gt,           // >
    Ge,           // >=
    Eq,           // =
    PlusEq,       // +=
    MinusEq,      // -=
    StarEq,       // *=
    SlashEq,      // /=
    PercentEq,    // %=
    ShiftLeftEq,  // <<=
    ShiftRightEq, // >>=
    AmpEq,        // &=
    PipeEq,       // |=
    CaretEq,      // ^=
    PlusPlus,     // ++
    MinusMinus,   // --
    Question,     // ?
    Colon,        // :
    Comma,        // ,
    LParen,       // (
    RParen,       // )
    Dollar,       // $
    Eof,
}

pub(super) struct ArithLexer {
    chars: Vec<char>,
    pos: usize,
}

impl ArithLexer {
    pub(super) fn new(input: &str) -> Self {
        ArithLexer {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    pub(super) fn next_token(&mut self) -> Result<ArithToken, String> {
        self.skip_whitespace();

        let ch = match self.peek_char() {
            Some(c) => c,
            None => return Ok(ArithToken::Eof),
        };

        // Numbers: decimal, octal (0NNN), hex (0xNNN)
        if ch.is_ascii_digit() {
            return self.lex_number();
        }

        // Identifiers (variable names)
        if ch.is_ascii_alphabetic() || ch == '_' {
            return Ok(self.lex_ident());
        }

        // Multi-character operators (check longest match first)
        match ch {
            '+' => {
                self.next_char();
                match self.peek_char() {
                    Some('+') => {
                        self.next_char();
                        Ok(ArithToken::PlusPlus)
                    }
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::PlusEq)
                    }
                    _ => Ok(ArithToken::Plus),
                }
            }
            '-' => {
                self.next_char();
                match self.peek_char() {
                    Some('-') => {
                        self.next_char();
                        Ok(ArithToken::MinusMinus)
                    }
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::MinusEq)
                    }
                    _ => Ok(ArithToken::Minus),
                }
            }
            '*' => {
                self.next_char();
                match self.peek_char() {
                    Some('*') => {
                        self.next_char();
                        Ok(ArithToken::StarStar)
                    }
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::StarEq)
                    }
                    _ => Ok(ArithToken::Star),
                }
            }
            '/' => {
                self.next_char();
                match self.peek_char() {
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::SlashEq)
                    }
                    _ => Ok(ArithToken::Slash),
                }
            }
            '%' => {
                self.next_char();
                match self.peek_char() {
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::PercentEq)
                    }
                    _ => Ok(ArithToken::Percent),
                }
            }
            '<' => {
                self.next_char();
                match self.peek_char() {
                    Some('<') => {
                        self.next_char();
                        match self.peek_char() {
                            Some('=') => {
                                self.next_char();
                                Ok(ArithToken::ShiftLeftEq)
                            }
                            _ => Ok(ArithToken::ShiftLeft),
                        }
                    }
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::Le)
                    }
                    _ => Ok(ArithToken::Lt),
                }
            }
            '>' => {
                self.next_char();
                match self.peek_char() {
                    Some('>') => {
                        self.next_char();
                        match self.peek_char() {
                            Some('=') => {
                                self.next_char();
                                Ok(ArithToken::ShiftRightEq)
                            }
                            _ => Ok(ArithToken::ShiftRight),
                        }
                    }
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::Ge)
                    }
                    _ => Ok(ArithToken::Gt),
                }
            }
            '&' => {
                self.next_char();
                match self.peek_char() {
                    Some('&') => {
                        self.next_char();
                        Ok(ArithToken::AmpAmp)
                    }
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::AmpEq)
                    }
                    _ => Ok(ArithToken::Amp),
                }
            }
            '|' => {
                self.next_char();
                match self.peek_char() {
                    Some('|') => {
                        self.next_char();
                        Ok(ArithToken::PipePipe)
                    }
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::PipeEq)
                    }
                    _ => Ok(ArithToken::Pipe),
                }
            }
            '^' => {
                self.next_char();
                match self.peek_char() {
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::CaretEq)
                    }
                    _ => Ok(ArithToken::Caret),
                }
            }
            '!' => {
                self.next_char();
                match self.peek_char() {
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::BangEq)
                    }
                    _ => Ok(ArithToken::Bang),
                }
            }
            '=' => {
                self.next_char();
                match self.peek_char() {
                    Some('=') => {
                        self.next_char();
                        Ok(ArithToken::EqEq)
                    }
                    _ => Ok(ArithToken::Eq),
                }
            }
            '~' => {
                self.next_char();
                Ok(ArithToken::Tilde)
            }
            '?' => {
                self.next_char();
                Ok(ArithToken::Question)
            }
            ':' => {
                self.next_char();
                Ok(ArithToken::Colon)
            }
            ',' => {
                self.next_char();
                Ok(ArithToken::Comma)
            }
            '(' => {
                self.next_char();
                Ok(ArithToken::LParen)
            }
            ')' => {
                self.next_char();
                Ok(ArithToken::RParen)
            }
            '$' => {
                self.next_char();
                Ok(ArithToken::Dollar)
            }
            _ => Err(format!(
                "unexpected character '{}' in arithmetic expression",
                ch
            )),
        }
    }

    fn lex_number(&mut self) -> Result<ArithToken, String> {
        let start = self.pos;

        // Check for hex (0x...) or octal (0...)
        if self.peek_char() == Some('0') {
            self.next_char();
            match self.peek_char() {
                Some('x') | Some('X') => {
                    // Hex
                    self.next_char();
                    let hex_start = self.pos;
                    while let Some(c) = self.peek_char() {
                        if c.is_ascii_hexdigit() {
                            self.next_char();
                        } else {
                            break;
                        }
                    }
                    if self.pos == hex_start {
                        return Err("invalid hex literal: no digits after 0x".to_string());
                    }
                    let hex_str: String = self.chars[hex_start..self.pos].iter().collect();
                    let value = i64::from_str_radix(&hex_str, 16)
                        .map_err(|e| format!("invalid hex literal: {}", e))?;
                    return Ok(ArithToken::Number(value));
                }
                Some(c) if c.is_ascii_digit() => {
                    // Octal
                    while let Some(c) = self.peek_char() {
                        if c.is_ascii_digit() {
                            self.next_char();
                        } else {
                            break;
                        }
                    }
                    let oct_str: String = self.chars[start + 1..self.pos].iter().collect();
                    let value = i64::from_str_radix(&oct_str, 8)
                        .map_err(|e| format!("invalid octal literal: {}", e))?;
                    return Ok(ArithToken::Number(value));
                }
                _ => {
                    // Just 0
                    return Ok(ArithToken::Number(0));
                }
            }
        }

        // Decimal
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                self.next_char();
            } else {
                break;
            }
        }
        let num_str: String = self.chars[start..self.pos].iter().collect();
        let value = num_str
            .parse::<i64>()
            .map_err(|e| format!("invalid number literal: {}", e))?;
        Ok(ArithToken::Number(value))
    }

    fn lex_ident(&mut self) -> ArithToken {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.next_char();
            } else {
                break;
            }
        }
        // Check for array subscript: ident[...]
        if self.peek_char() == Some('[') {
            self.next_char(); // consume [
            let mut depth = 1;
            while let Some(c) = self.peek_char() {
                self.next_char();
                match c {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        ArithToken::Ident(name)
    }
}
