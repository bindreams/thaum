use contracts::debug_requires;

use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::token::{SpannedToken, Token};

/// Opaque checkpoint handle. Can only be created by `TokenStream::checkpoint()`.
/// Consumed by `rewind()` or `release()` — the compiler enforces single use.
pub(super) struct Checkpoint(usize);

/// Buffered token stream with checkpoint/rewind for speculative parsing.
///
/// Call `skip_blanks()` to consume `Blank` tokens before peeking/advancing.
/// Most parser code calls `skip_blanks()` via Parser helper methods (`eat`,
/// `expect`, `is_word`, etc.). Word collection code deliberately skips the
/// call to see `Blank` tokens as word boundaries.
pub(crate) struct TokenStream<'src> {
    lexer: Lexer<'src>,
    buffer: Vec<SpannedToken>,
    pos: usize,
    earliest_checkpoint: usize,
}

impl<'src> TokenStream<'src> {
    pub(super) fn new(lexer: Lexer<'src>) -> Result<Self, ParseError> {
        let mut stream = TokenStream {
            lexer,
            buffer: Vec::new(),
            pos: 0,
            earliest_checkpoint: usize::MAX,
        };
        stream.ensure_buffered()?;
        Ok(stream)
    }

    /// Consume all `Blank` tokens at the current position.
    ///
    /// Call this before `peek()`/`advance()` when you want to skip word
    /// boundaries. Word collection code omits this call to see where
    /// one word ends and the next begins.
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
    /// offset=0 is the current token, offset=1 is the next, etc.
    pub(super) fn peek_at_offset(
        &mut self,
        offset: usize,
    ) -> Result<&SpannedToken, ParseError> {
        let target = self.pos + offset;
        self.ensure_buffered_at(target)?;
        Ok(&self.buffer[target])
    }

    // === Checkpoint/rewind ===

    /// Save current position for potential rewind.
    pub(super) fn checkpoint(&mut self) -> Checkpoint {
        let saved = self.pos;
        self.earliest_checkpoint = self.earliest_checkpoint.min(saved);
        Checkpoint(saved)
    }

    /// Rewind to a saved position. Consumes the checkpoint.
    #[debug_requires(cp.0 <= self.pos, "can't rewind to the future")]
    pub(super) fn rewind(&mut self, cp: Checkpoint) {
        self.pos = cp.0;
    }

    /// Release a checkpoint, allowing buffer cleanup. Consumes the checkpoint.
    #[debug_requires(cp.0 <= self.pos, "can't release a future checkpoint")]
    pub(super) fn release(&mut self, cp: Checkpoint) {
        if cp.0 <= self.earliest_checkpoint {
            self.earliest_checkpoint = self.pos;
            if self.pos > 0 {
                self.buffer.drain(..self.pos);
                self.pos = 0;
                self.earliest_checkpoint = 0;
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
