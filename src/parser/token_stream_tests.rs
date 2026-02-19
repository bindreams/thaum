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
    assert_eq!(s.peek().unwrap().token, Token::Word("echo".into()));
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
fn advance_moves_forward() {
    let mut s = make_stream("echo hello");
    s.advance().unwrap(); // consume "echo"
    assert_eq!(s.peek().unwrap().token, Token::Word("hello".into()));
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
    s.advance().unwrap(); // consume "b"
    assert_eq!(s.peek().unwrap().token, Token::Word("c".into()));
    s.rewind(cp);
    assert_eq!(s.peek().unwrap().token, Token::Word("a".into()));
}

#[test]
fn rewind_allows_replay() {
    let mut s = make_stream("a b");
    let cp = s.checkpoint();
    let t1 = s.advance().unwrap().token;
    let t2 = s.advance().unwrap().token;
    s.rewind(cp);
    assert_eq!(s.advance().unwrap().token, t1);
    assert_eq!(s.advance().unwrap().token, t2);
}

#[test]
fn nested_checkpoints() {
    let mut s = make_stream("a b c d");
    let cp_a = s.checkpoint();
    s.advance().unwrap(); // consume "a"
    let cp_b = s.checkpoint();
    s.advance().unwrap(); // consume "b"
                          // Rewind to B — should see "b"
    s.rewind(cp_b);
    assert_eq!(s.peek().unwrap().token, Token::Word("b".into()));
    // Rewind to A — should see "a"
    s.rewind(cp_a);
    assert_eq!(s.peek().unwrap().token, Token::Word("a".into()));
}

#[test]
fn rewind_then_advance_past_original() {
    let mut s = make_stream("a b c d");
    let cp = s.checkpoint();
    s.advance().unwrap(); // "a"
    s.rewind(cp);
    // Now advance past where we were before
    s.advance().unwrap(); // "a"
    s.advance().unwrap(); // "b"
    s.advance().unwrap(); // "c"
    assert_eq!(s.peek().unwrap().token, Token::Word("d".into()));
}

// --- Release / buffer cleanup ---

#[test]
fn release_allows_buffer_drain() {
    let mut s = make_stream("a b c d e");
    let cp = s.checkpoint();
    for _ in 0..5 {
        s.advance().unwrap();
    }
    let buf_before = s.buffer.len();
    s.release(cp);
    // Buffer should have been drained
    assert!(s.buffer.len() < buf_before);
}

#[test]
fn release_with_older_checkpoint_alive() {
    let mut s = make_stream("a b c d e");
    let _cp_old = s.checkpoint();
    s.advance().unwrap(); // "a"
    s.advance().unwrap(); // "b"
    let cp_new = s.checkpoint();
    s.advance().unwrap(); // "c"
    let buf_len = s.buffer.len();
    // Release the newer checkpoint — old one still alive
    s.release(cp_new);
    // Buffer should NOT have been drained
    assert_eq!(s.buffer.len(), buf_len);
    // Note: cp_old is deliberately leaked here — we're testing that
    // releasing a newer checkpoint doesn't drain when an older exists.
}

#[test]
fn release_oldest_frees_buffer() {
    let mut s = make_stream("a b c d e");
    let cp = s.checkpoint();
    for _ in 0..5 {
        s.advance().unwrap();
    }
    s.release(cp);
    // After release, pos should be 0 (rebased)
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
