use contracts::debug_requires;

use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::token::SpannedToken;

/// Opaque checkpoint handle. Can only be created by `TokenStream::checkpoint()`.
/// Consumed by `rewind()` or `release()` — the compiler enforces single use.
pub(super) struct Checkpoint(usize);

/// Buffered token stream with checkpoint/rewind for speculative parsing.
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
        // Pre-buffer the first token so peek() always works
        stream.ensure_buffered()?;
        Ok(stream)
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
            // Drain buffer entries before current pos
            if self.pos > 0 {
                self.buffer.drain(..self.pos);
                self.pos = 0;
                // Reset earliest_checkpoint relative to new buffer start
                self.earliest_checkpoint = 0;
            }
        }
    }

    /// Ensure the buffer has a token at the current `pos`.
    fn ensure_buffered(&mut self) -> Result<(), ParseError> {
        while self.pos >= self.buffer.len() {
            let tok = self.lexer.next_token()?;
            self.buffer.push(tok);
        }
        Ok(())
    }
}


#[cfg(test)]
#[path = "token_stream_tests.rs"]
mod token_stream_tests;
