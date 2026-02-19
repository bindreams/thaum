use super::TokenStream;
use crate::dialect::ParseOptions;
use crate::lexer::Lexer;
use crate::token::Token;

fn make_stream(input: &str) -> TokenStream<'_> {
    let lexer = Lexer::new(input, ParseOptions::default());
    TokenStream::new(lexer).unwrap()
}

// --- Basic operations ---

#[test]
fn peek_returns_first_token() {
    let mut s = make_stream("echo hello");
    assert_eq!(s.peek().unwrap().token, Token::Literal("echo".into()));
}

#[test]
fn peek_is_idempotent() {
    let mut s = make_stream("echo hello");
    let t1 = s.peek().unwrap().token.clone();
    let t2 = s.peek().unwrap().token.clone();
    assert_eq!(t1, t2);
}

#[test]
fn advance_returns_peeked() {
    let mut s = make_stream("echo hello");
    let peeked = s.peek().unwrap().token.clone();
    let advanced = s.advance().unwrap().token;
    assert_eq!(peeked, advanced);
}

#[test]
fn advance_then_skip_blanks() {
    let mut s = make_stream("echo hello");
    s.advance().unwrap(); // consume "echo"
    s.skip_blanks().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Literal("hello".into()));
}

#[test]
fn advance_past_eof() {
    let mut s = make_stream("x");
    s.advance().unwrap(); // consume "x"
    assert_eq!(s.peek().unwrap().token, Token::Eof);
    assert_eq!(s.advance().unwrap().token, Token::Eof);
    assert_eq!(s.advance().unwrap().token, Token::Eof);
}

// --- Checkpoint / rewind ---

#[test]
fn rewind_restores_position() {
    let mut s = make_stream("a b c");
    let cp = s.checkpoint();
    s.advance().unwrap(); // consume "a"
    s.skip_blanks().unwrap();
    s.advance().unwrap(); // consume "b"
    s.skip_blanks().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Literal("c".into()));
    s.rewind(cp);
    assert_eq!(s.peek().unwrap().token, Token::Literal("a".into()));
}

#[test]
fn rewind_allows_replay() {
    let mut s = make_stream("a b");
    let cp = s.checkpoint();
    let t1 = s.advance().unwrap().token;
    s.skip_blanks().unwrap();
    let t2 = s.advance().unwrap().token;
    s.rewind(cp);
    assert_eq!(s.advance().unwrap().token, t1);
    s.skip_blanks().unwrap();
    assert_eq!(s.advance().unwrap().token, t2);
}

#[test]
fn nested_checkpoints() {
    let mut s = make_stream("a b c d");
    let cp_a = s.checkpoint();
    s.advance().unwrap(); // consume "a"
    s.skip_blanks().unwrap();
    let cp_b = s.checkpoint();
    s.advance().unwrap(); // consume "b"
    s.rewind(cp_b);
    assert_eq!(s.peek().unwrap().token, Token::Literal("b".into()));
    s.rewind(cp_a);
    assert_eq!(s.peek().unwrap().token, Token::Literal("a".into()));
}

#[test]
fn rewind_then_advance_past_original() {
    let mut s = make_stream("a b c d");
    let cp = s.checkpoint();
    s.advance().unwrap(); // "a"
    s.rewind(cp);
    s.advance().unwrap(); // "a"
    s.skip_blanks().unwrap();
    s.advance().unwrap(); // "b"
    s.skip_blanks().unwrap();
    s.advance().unwrap(); // "c"
    s.skip_blanks().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Literal("d".into()));
}

// --- Release / buffer cleanup ---

#[test]
fn release_allows_buffer_drain() {
    let mut s = make_stream("a b c d e");
    let cp = s.checkpoint();
    for _ in 0..5 {
        s.skip_blanks().unwrap();
        s.advance().unwrap();
    }
    let buf_before = s.buffer.len();
    s.release(cp);
    assert!(s.buffer.len() < buf_before);
}

#[test]
fn release_with_older_checkpoint_alive() {
    let mut s = make_stream("a b c d e");
    let _cp_old = s.checkpoint();
    s.advance().unwrap(); // "a"
    s.skip_blanks().unwrap();
    s.advance().unwrap(); // "b"
    s.skip_blanks().unwrap();
    let cp_new = s.checkpoint();
    s.advance().unwrap(); // "c"
    let buf_len = s.buffer.len();
    s.release(cp_new);
    assert_eq!(s.buffer.len(), buf_len);
}

#[test]
fn release_oldest_frees_buffer() {
    let mut s = make_stream("a b c d e");
    let cp = s.checkpoint();
    for _ in 0..5 {
        s.skip_blanks().unwrap();
        s.advance().unwrap();
    }
    s.release(cp);
    assert_eq!(s.pos, 0);
}

// --- Edge cases ---

#[test]
fn empty_input() {
    let mut s = make_stream("");
    assert_eq!(s.peek().unwrap().token, Token::Eof);
}

#[test]
fn peek_after_rewind() {
    let mut s = make_stream("x y");
    let first = s.peek().unwrap().token.clone();
    let cp = s.checkpoint();
    s.advance().unwrap();
    s.rewind(cp);
    assert_eq!(s.peek().unwrap().token, first);
}

// --- skip_blanks ---

#[test]
fn peek_sees_blank_without_skip() {
    let mut s = make_stream("a b");
    s.advance().unwrap(); // consume "a"
    // Without skip_blanks, peek sees Blank
    assert_eq!(s.peek().unwrap().token, Token::Blank);
}

#[test]
fn skip_blanks_then_peek() {
    let mut s = make_stream("a b");
    s.advance().unwrap(); // consume "a"
    s.skip_blanks().unwrap();
    // After skip_blanks, peek sees "b"
    assert_eq!(s.peek().unwrap().token, Token::Literal("b".into()));
}
