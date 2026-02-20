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
    s.advance().unwrap();
    s.skip_blanks().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Literal("hello".into()));
}

#[test]
fn advance_past_eof() {
    let mut s = make_stream("x");
    s.advance().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Eof);
    assert_eq!(s.advance().unwrap().token, Token::Eof);
}

// --- skip_blanks ---

#[test]
fn peek_sees_blank_without_skip() {
    let mut s = make_stream("a b");
    s.advance().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Blank);
}

#[test]
fn skip_blanks_then_peek() {
    let mut s = make_stream("a b");
    s.advance().unwrap();
    s.skip_blanks().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Literal("b".into()));
}

// --- speculate ---

#[test]
fn speculate_rewinds_on_none() {
    let mut s = make_stream("a b c");
    let result: Option<()> = s.speculate(|s| {
        s.advance()?;
        s.skip_blanks()?;
        s.advance()?;
        Ok(None)
    }).unwrap();
    assert!(result.is_none());
    assert_eq!(s.peek().unwrap().token, Token::Literal("a".into()));
}

#[test]
fn speculate_keeps_position_on_some() {
    let mut s = make_stream("a b c");
    let result = s.speculate(|s| {
        s.advance()?;
        s.skip_blanks()?;
        Ok(Some("found"))
    }).unwrap();
    assert_eq!(result, Some("found"));
    assert_eq!(s.peek().unwrap().token, Token::Literal("b".into()));
}

// --- Edge cases ---

#[test]
fn empty_input() {
    let mut s = make_stream("");
    assert_eq!(s.peek().unwrap().token, Token::Eof);
}
