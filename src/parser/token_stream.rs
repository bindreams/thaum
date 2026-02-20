use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::token::{SpannedToken, Token};

/// Buffered token stream with speculative parsing support.
///
/// Call `skip_blanks()` to consume `Blank` tokens before peeking/advancing.
/// Most parser code calls `skip_blanks()` via Parser helper methods (`eat`,
/// `expect`, `is_word`, etc.). Word collection code deliberately skips the
/// call to see `Blank` tokens as word boundaries.
pub(crate) struct TokenStream<'src> {
    lexer: Lexer<'src>,
    buffer: Vec<SpannedToken>,
    pos: usize,
}

impl<'src> TokenStream<'src> {
    pub(super) fn new(lexer: Lexer<'src>) -> Result<Self, ParseError> {
        let mut stream = TokenStream {
            lexer,
            buffer: Vec::new(),
            pos: 0,
        };
        stream.ensure_buffered()?;
        Ok(stream)
    }

    /// Consume all `Blank` tokens at the current position.
    pub(super) fn skip_blanks(&mut self) -> Result<(), ParseError> {
        loop {
            self.ensure_buffered()?;
            if self.buffer[self.pos].token == Token::Blank {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Look at the next token without consuming it.
    pub(super) fn peek(&mut self) -> Result<&SpannedToken, ParseError> {
        self.ensure_buffered()?;
        Ok(&self.buffer[self.pos])
    }

    /// Consume and return the next token.
    pub(super) fn advance(&mut self) -> Result<SpannedToken, ParseError> {
        self.ensure_buffered()?;
        let tok = self.buffer[self.pos].clone();
        self.pos += 1;
        Ok(tok)
    }

    /// Peek at a token at an offset from the current position.
    pub(super) fn peek_at_offset(
        &mut self,
        offset: usize,
    ) -> Result<&SpannedToken, ParseError> {
        let target = self.pos + offset;
        self.ensure_buffered_at(target)?;
        Ok(&self.buffer[target])
    }

    /// Try a speculative parse. Saves the current position, runs the
    /// closure, and rewinds if the closure returns `None`.
    pub(super) fn speculate<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<Option<T>, ParseError>,
    ) -> Result<Option<T>, ParseError> {
        let saved = self.pos;
        match f(self)? {
            Some(v) => Ok(Some(v)),
            None => {
                self.pos = saved;
                Ok(None)
            }
        }
    }

    // === Internal ===

    fn ensure_buffered(&mut self) -> Result<(), ParseError> {
        while self.pos >= self.buffer.len() {
            let tok = self.lexer.next_token()?;
            self.buffer.push(tok);
        }
        Ok(())
    }

    fn ensure_buffered_at(&mut self, target: usize) -> Result<(), ParseError> {
        while target >= self.buffer.len() {
            let tok = self.lexer.next_token()?;
            self.buffer.push(tok);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "token_stream_tests.rs"]
mod token_stream_tests;
