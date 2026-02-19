use super::*;

#[test]
fn pattern_match_literal() {
    assert!(shell_pattern_match("hello", "hello"));
    assert!(!shell_pattern_match("hello", "world"));
}

#[test]
fn pattern_match_star() {
    assert!(shell_pattern_match("hello", "*"));
    assert!(shell_pattern_match("hello", "hel*"));
    assert!(shell_pattern_match("hello", "*llo"));
    assert!(shell_pattern_match("hello", "h*o"));
    assert!(!shell_pattern_match("hello", "h*x"));
}

#[test]
fn pattern_match_question() {
    assert!(shell_pattern_match("hello", "hell?"));
    assert!(shell_pattern_match("hello", "?ello"));
    assert!(!shell_pattern_match("hello", "hell"));
}

#[test]
fn pattern_match_bracket() {
    assert!(shell_pattern_match("hello", "[h]ello"));
    assert!(shell_pattern_match("hello", "[a-z]ello"));
    assert!(!shell_pattern_match("hello", "[A-Z]ello"));
    assert!(shell_pattern_match("hello", "[!A-Z]ello"));
}
