//! Here-document body reading and delimiter quote stripping. Bodies are read
//! autonomously by the lexer on newline and queued for the parser to consume.

use crate::error::LexError;
use crate::span::Span;

use super::Lexer;

/// Pending here-document to be read after the next newline.
#[derive(Debug)]
pub(super) struct PendingHereDoc {
    pub(super) delimiter: String,
    pub(super) strip_tabs: bool,
}

impl Lexer {
    /// Read a single here-document body until the delimiter line is found.
    pub(super) fn read_single_heredoc(&mut self, delimiter: &str, strip_tabs: bool) -> Result<String, LexError> {
        let start = self.cursor_pos().0;
        let mut body = String::new();

        loop {
            if self.is_at_eof() {
                return Err(LexError::UnterminatedHereDoc {
                    delimiter: delimiter.to_string(),
                    span: Span::new(start, self.cursor_pos().0),
                });
            }

            // Read one line
            let mut line = String::new();
            loop {
                match self.advance_char() {
                    Some('\n') => break,
                    Some(c) => line.push(c),
                    None => break,
                }
            }

            // Check if this line matches the delimiter
            let check_line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                &line
            };

            if check_line == delimiter {
                break;
            }

            // Add line to body
            if strip_tabs {
                body.push_str(line.trim_start_matches('\t'));
            } else {
                body.push_str(&line);
            }
            body.push('\n');
        }

        Ok(body)
    }
}

/// Process a raw heredoc delimiter word to extract the actual delimiter
/// and detect whether it was quoted.
///
/// In shell, the delimiter in `<<'EOF'` or `<<"EOF"` or `<<\EOF` is quoted,
/// meaning the heredoc body should not undergo variable expansion.
pub(crate) fn strip_heredoc_quotes(raw: &str) -> (String, bool) {
    let mut result = String::new();
    let mut quoted = false;
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                quoted = true;
                while let Some(&c) = chars.peek() {
                    if c == '\'' {
                        chars.next();
                        break;
                    }
                    result.push(c);
                    chars.next();
                }
            }
            '"' => {
                quoted = true;
                while let Some(&c) = chars.peek() {
                    if c == '"' {
                        chars.next();
                        break;
                    }
                    if c == '\\' {
                        chars.next();
                        if let Some(&next) = chars.peek() {
                            result.push(next);
                            chars.next();
                        }
                    } else {
                        result.push(c);
                        chars.next();
                    }
                }
            }
            '\\' => {
                quoted = true;
                if let Some(c) = chars.next() {
                    result.push(c);
                }
            }
            _ => result.push(ch),
        }
    }

    (result, quoted)
}
