use super::*;

#[skuld::test]
fn span_new() {
    let s = Span::new(5, 10);
    assert_eq!(s.start, BytePos(5));
    assert_eq!(s.end, BytePos(10));
}

#[skuld::test]
fn span_merge_adjacent() {
    let a = Span::new(0, 5);
    let b = Span::new(5, 10);
    let merged = a.merge(b);
    assert_eq!(merged, Span::new(0, 10));
}

#[skuld::test]
fn span_merge_overlapping() {
    let a = Span::new(2, 8);
    let b = Span::new(5, 12);
    let merged = a.merge(b);
    assert_eq!(merged, Span::new(2, 12));
}

#[skuld::test]
fn span_merge_reversed_order() {
    let a = Span::new(10, 20);
    let b = Span::new(0, 5);
    let merged = a.merge(b);
    assert_eq!(merged, Span::new(0, 20));
}

#[skuld::test]
fn span_empty() {
    let s = Span::empty(7);
    assert_eq!(s.start, BytePos(7));
    assert_eq!(s.end, BytePos(7));
}
