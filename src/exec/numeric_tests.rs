use super::*;

skuld::default_labels!(exec);

#[skuld::test]
fn test_empty() {
    assert_eq!(parse_shell_int(""), Ok(0));
}

#[skuld::test]
fn test_whitespace() {
    assert_eq!(parse_shell_int("  "), Ok(0));
}

#[skuld::test]
fn test_decimal() {
    assert_eq!(parse_shell_int("42"), Ok(42));
}

#[skuld::test]
fn test_negative() {
    assert_eq!(parse_shell_int("-7"), Ok(-7));
}

#[skuld::test]
fn test_hex() {
    assert_eq!(parse_shell_int("0xff"), Ok(255));
    assert_eq!(parse_shell_int("0XFF"), Ok(255));
}

#[skuld::test]
fn test_octal() {
    assert_eq!(parse_shell_int("077"), Ok(63));
}

#[skuld::test]
fn test_char_code() {
    assert_eq!(parse_shell_int("'A"), Ok(65));
    assert_eq!(parse_shell_int("\"A"), Ok(65));
}

#[skuld::test]
fn test_invalid() {
    assert_eq!(parse_shell_int("abc"), Err(()));
}
