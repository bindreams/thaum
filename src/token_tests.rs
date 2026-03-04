use super::*;

#[skuld::test]
fn fragment_tokens_recognized() {
    assert!(Token::Literal("x".into()).is_fragment());
    assert!(Token::SingleQuoted("x".into()).is_fragment());
    assert!(Token::SimpleParam("x".into()).is_fragment());
    assert!(Token::Glob(GlobKind::Star).is_fragment());
    assert!(!Token::Pipe.is_fragment());
    assert!(!Token::Whitespace.is_fragment());
    assert!(!Token::Newline.is_fragment());
    assert!(!Token::Eof.is_fragment());
}

#[skuld::test]
fn redirect_ops_recognized() {
    assert!(Token::RedirectFromFile.is_redirect_op());
    assert!(Token::RedirectToFile.is_redirect_op());
    assert!(Token::Append.is_redirect_op());
    assert!(Token::Clobber.is_redirect_op());
    assert!(!Token::Pipe.is_redirect_op());
    assert!(!Token::Semicolon.is_redirect_op());
}
