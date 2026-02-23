//! Byte-offset source spans attached to every AST node and token.

/// A byte offset into the source string.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]

pub struct BytePos(pub usize);

/// A contiguous byte range in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]

pub struct Span {
    pub start: BytePos,
    pub end: BytePos,
}

impl Span {
    /// Create a span from byte offsets (inclusive start, exclusive end).
    pub fn new(start: usize, end: usize) -> Self {
        Span {
            start: BytePos(start),
            end: BytePos(end),
        }
    }

    /// Merge two spans into one that covers both.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: BytePos(self.start.0.min(other.start.0)),
            end: BytePos(self.end.0.max(other.end.0)),
        }
    }

    /// Create a zero-width span at a single byte position.
    pub fn empty(pos: usize) -> Self {
        Span::new(pos, pos)
    }
}

#[cfg(test)]
#[path = "span_tests.rs"]
mod tests;
