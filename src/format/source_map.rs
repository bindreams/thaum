//! Byte-offset to line/column mapper for source-location display.

use crate::span::Span;

/// Maps byte offsets to line/column positions for human-readable error messages.
///
/// Pre-computes line-start offsets in a single pass over the source text.
pub struct SourceMapper {
    /// Byte offset of the start of each line (0-indexed).
    line_starts: Vec<usize>,
}

impl SourceMapper {
    /// Build a mapper by scanning the source for newline positions.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(i + 1);
            }
        }
        SourceMapper { line_starts }
    }

    /// Convert a byte offset to (line, column), both 1-based.
    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let col = offset - self.line_starts[line];
        (line + 1, col + 1)
    }

    /// Format a span as `filename:line:col` for use in YAML source annotations.
    pub fn format_span(&self, span: Span, filename: &str) -> String {
        let (line, col) = self.offset_to_line_col(span.start.0);
        format!("{}:{}:{}", filename, line, col)
    }
}
