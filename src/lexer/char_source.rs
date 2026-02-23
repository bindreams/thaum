//! Forward-only character source backed by `Box<dyn Read>` with arbitrary
//! lookahead via `peek_at(n)`. Uses `RefCell` for interior mutability so
//! peek operations take `&self`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Cursor, Read};

use crate::error::LexError;

/// I/O internals that are mutated by peek operations (interior mutability).
struct CharSourceInner {
    reader: BufReader<Box<dyn Read>>,
    lookahead: VecDeque<char>,
    eof: bool,
}

/// Forward-only character source backed by any `Read` implementation.
///
/// Provides character-level peek and advance with arbitrary lookahead.
/// Characters are decoded from UTF-8 on demand and buffered in a VecDeque.
/// `byte_pos` tracks the byte offset for span construction.
///
/// Peek operations take `&self` via interior mutability — they are logically
/// pure (same result if called twice with no advance in between).
pub(crate) struct CharSource {
    inner: RefCell<CharSourceInner>,
    byte_pos: usize,
}

impl CharSource {
    /// Create a CharSource from a string (copies the bytes).
    pub(crate) fn from_str(s: &str) -> Self {
        Self::from_reader(Cursor::new(s.as_bytes().to_vec()))
    }

    /// Create a CharSource from any Read implementation.
    pub(crate) fn from_reader(reader: impl Read + 'static) -> Self {
        CharSource {
            inner: RefCell::new(CharSourceInner {
                reader: BufReader::new(Box::new(reader)),
                lookahead: VecDeque::new(),
                eof: false,
            }),
            byte_pos: 0,
        }
    }

    /// Peek at the next character without consuming it.
    pub(crate) fn peek(&self) -> Option<char> {
        let mut inner = self.inner.borrow_mut();
        fill(&mut inner, 1);
        inner.lookahead.front().copied()
    }

    /// Peek at the character at offset `n` from current position (0 = next char).
    pub(crate) fn peek_at(&self, n: usize) -> Option<char> {
        let mut inner = self.inner.borrow_mut();
        fill(&mut inner, n + 1);
        inner.lookahead.get(n).copied()
    }

    /// Consume and return the next character.
    pub(crate) fn advance(&mut self) -> Option<char> {
        let inner = self.inner.get_mut();
        fill(inner, 1);
        let ch = inner.lookahead.pop_front()?;
        self.byte_pos += ch.len_utf8();
        Some(ch)
    }

    /// Current byte position in the input stream.
    pub(crate) fn byte_pos(&self) -> usize {
        self.byte_pos
    }

    /// Whether the source is exhausted.
    pub(crate) fn is_eof(&self) -> bool {
        let mut inner = self.inner.borrow_mut();
        fill(&mut inner, 1);
        inner.lookahead.is_empty()
    }
}

/// Ensure at least `n` characters are in the lookahead buffer.
fn fill(inner: &mut CharSourceInner, n: usize) {
    while inner.lookahead.len() < n && !inner.eof {
        match read_char(inner) {
            Ok(Some(ch)) => inner.lookahead.push_back(ch),
            Ok(None) => {
                inner.eof = true;
            }
            Err(_) => {
                // TODO: propagate IO errors properly
                inner.eof = true;
            }
        }
    }
}

/// Read and decode one UTF-8 character from the BufReader.
fn read_char(inner: &mut CharSourceInner) -> Result<Option<char>, LexError> {
    let buf = inner
        .reader
        .fill_buf()
        .map_err(|e| LexError::Io(e.to_string()))?;
    if buf.is_empty() {
        return Ok(None);
    }

    let first = buf[0];
    let char_len = match first {
        0..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => return Err(LexError::Io("invalid UTF-8 start byte".to_string())),
    };

    if buf.len() >= char_len {
        // All bytes available in the buffer
        let s = std::str::from_utf8(&buf[..char_len]).map_err(|e| LexError::Io(e.to_string()))?;
        let ch = s.chars().next().unwrap();
        inner.reader.consume(char_len);
        Ok(Some(ch))
    } else {
        // Partial character at buffer boundary — read byte by byte
        let mut bytes = [0u8; 4];
        bytes[0] = first;
        inner.reader.consume(1);
        for byte in bytes.iter_mut().take(char_len).skip(1) {
            let b = inner
                .reader
                .fill_buf()
                .map_err(|e| LexError::Io(e.to_string()))?;
            if b.is_empty() {
                return Err(LexError::Io("truncated UTF-8 sequence".to_string()));
            }
            *byte = b[0];
            inner.reader.consume(1);
        }
        let s = std::str::from_utf8(&bytes[..char_len]).map_err(|e| LexError::Io(e.to_string()))?;
        let ch = s.chars().next().unwrap();
        Ok(Some(ch))
    }
}
