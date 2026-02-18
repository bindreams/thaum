/// A byte offset into the source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]

pub struct BytePos(pub usize);

/// A contiguous byte range in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub struct Span {
    pub start: BytePos,
    pub end: BytePos,
}

impl Span {
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

    pub fn empty(pos: usize) -> Self {
        Span::new(pos, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_new() {
        let s = Span::new(5, 10);
        assert_eq!(s.start, BytePos(5));
        assert_eq!(s.end, BytePos(10));
    }

    #[test]
    fn span_merge_adjacent() {
        let a = Span::new(0, 5);
        let b = Span::new(5, 10);
        let merged = a.merge(b);
        assert_eq!(merged, Span::new(0, 10));
    }

    #[test]
    fn span_merge_overlapping() {
        let a = Span::new(2, 8);
        let b = Span::new(5, 12);
        let merged = a.merge(b);
        assert_eq!(merged, Span::new(2, 12));
    }

    #[test]
    fn span_merge_reversed_order() {
        let a = Span::new(10, 20);
        let b = Span::new(0, 5);
        let merged = a.merge(b);
        assert_eq!(merged, Span::new(0, 20));
    }

    #[test]
    fn span_empty() {
        let s = Span::empty(7);
        assert_eq!(s.start, BytePos(7));
        assert_eq!(s.end, BytePos(7));
    }
}
