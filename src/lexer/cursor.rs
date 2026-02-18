use crate::span::BytePos;

/// Low-level character-by-character cursor over source text.
pub(super) struct Cursor<'src> {
    pub(super) source: &'src str,
    pub(super) pos: usize,
}

impl<'src> Cursor<'src> {
    pub(super) fn new(source: &'src str) -> Self {
        Cursor { source, pos: 0 }
    }

    pub(super) fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    pub(super) fn peek_second(&self) -> Option<char> {
        let mut chars = self.source[self.pos..].chars();
        chars.next();
        chars.next()
    }

    pub(super) fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    pub(super) fn pos(&self) -> BytePos {
        BytePos(self.pos)
    }

    pub(super) fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }
}
