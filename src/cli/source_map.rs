use shell_parser::span::Span;

pub(super) struct SourceMapper {
    /// Byte offset of the start of each line (0-indexed).
    line_starts: Vec<usize>,
}

impl SourceMapper {
    pub(super) fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(i + 1);
            }
        }
        SourceMapper { line_starts }
    }

    /// Convert a byte offset to (line, column), both 1-based.
    pub(super) fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let col = offset - self.line_starts[line];
        (line + 1, col + 1)
    }

    pub(super) fn format_span(&self, span: Span, filename: &str) -> String {
        let (line, col) = self.offset_to_line_col(span.start.0);
        format!("{}:{}:{}", filename, line, col)
    }
}
