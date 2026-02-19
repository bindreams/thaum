use super::*;

#[test]
fn reserved_words_recognized() {
    assert!(Token::If.is_reserved_word());
    assert!(Token::Then.is_reserved_word());
    assert!(Token::Done.is_reserved_word());
    assert!(Token::Bang.is_reserved_word());
    assert!(!Token::Pipe.is_reserved_word());
    assert!(!Token::Word("if".into()).is_reserved_word());
}

#[test]
fn redirect_ops_recognized() {
    assert!(Token::RedirectFromFile.is_redirect_op());
    assert!(Token::RedirectToFile.is_redirect_op());
    assert!(Token::Append.is_redirect_op());
    assert!(Token::Clobber.is_redirect_op());
    assert!(!Token::Pipe.is_redirect_op());
    assert!(!Token::Semicolon.is_redirect_op());
}

#[test]
fn reserved_word_from_str_works() {
    assert_eq!(Token::reserved_word_from_str("if"), Some(Token::If));
    assert_eq!(Token::reserved_word_from_str("done"), Some(Token::Done));
    assert_eq!(Token::reserved_word_from_str("{"), Some(Token::LBrace));
    assert_eq!(Token::reserved_word_from_str("!"), Some(Token::Bang));
    assert_eq!(Token::reserved_word_from_str("echo"), None);
    assert_eq!(Token::reserved_word_from_str(""), None);
}
