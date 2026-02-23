use super::*;

fn fmt(format: &str, args: &[&str]) -> String {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut buf = Vec::new();
    printf_format(format, &args, &mut buf);
    String::from_utf8(buf).unwrap()
}

#[test]
fn test_basic_string() {
    assert_eq!(fmt("%s", &["hello"]), "hello");
}

#[test]
fn test_basic_int() {
    assert_eq!(fmt("%d", &["42"]), "42");
}

#[test]
fn test_hex() {
    assert_eq!(fmt("%x", &["255"]), "ff");
    assert_eq!(fmt("%X", &["255"]), "FF");
}

#[test]
fn test_octal() {
    assert_eq!(fmt("%o", &["8"]), "10");
}

#[test]
fn test_escape_newline() {
    assert_eq!(fmt("a\\nb", &[]), "a\nb");
}

#[test]
fn test_escape_hex() {
    assert_eq!(fmt("\\x41", &[]), "A");
}

#[test]
fn test_cyclic() {
    assert_eq!(fmt("%s\\n", &["a", "b", "c"]), "a\nb\nc\n");
}

#[test]
fn test_missing_arg_string() {
    assert_eq!(fmt("%s|%s", &["hello"]), "hello|");
}

#[test]
fn test_missing_arg_int() {
    assert_eq!(fmt("%d", &[]), "0");
}

#[test]
fn test_width_string() {
    assert_eq!(fmt("[%10s]", &["hi"]), "[        hi]");
}

#[test]
fn test_left_align() {
    assert_eq!(fmt("[%-10s]", &["hi"]), "[hi        ]");
}

#[test]
fn test_zero_pad() {
    assert_eq!(fmt("[%05d]", &["42"]), "[00042]");
}

#[test]
fn test_precision_string() {
    assert_eq!(fmt("[%.3s]", &["hello"]), "[hel]");
}

#[test]
fn test_precision_int() {
    assert_eq!(fmt("[%6.4d]", &["42"]), "[  0042]");
}

#[test]
fn test_float() {
    assert_eq!(fmt("[%.2f]", &["3.14159"]), "[3.14]");
}

#[test]
fn test_percent_literal() {
    assert_eq!(fmt("%%", &[]), "%");
}

#[test]
fn test_hex_arg() {
    assert_eq!(fmt("%d", &["0xff"]), "255");
}

#[test]
fn test_octal_arg() {
    assert_eq!(fmt("%d", &["077"]), "63");
}

#[test]
fn test_char_arg() {
    assert_eq!(fmt("%d", &["'A"]), "65");
}

#[test]
fn test_hash_hex() {
    assert_eq!(fmt("%#x", &["255"]), "0xff");
}

#[test]
fn test_hash_octal() {
    assert_eq!(fmt("%#o", &["8"]), "010");
}

#[test]
fn test_char_conv() {
    assert_eq!(fmt("%c", &["hello"]), "h");
}

#[test]
fn test_negative_zero_pad() {
    assert_eq!(fmt("[%010d]", &["-42"]), "[-000000042]");
}

#[test]
fn test_shell_quote_safe() {
    assert_eq!(fmt("%q", &["hello"]), "hello");
}

#[test]
fn test_shell_quote_special() {
    let result = fmt("%q", &["hello world"]);
    assert!(result.contains("hello") && result.contains("world"));
    assert_ne!(result, "hello world");
}

#[test]
fn test_backslash_b() {
    assert_eq!(fmt("%b", &["a\\nb"]), "a\nb");
}

#[test]
fn test_unsigned() {
    assert_eq!(fmt("%u", &["42"]), "42");
}

#[test]
fn test_parse_int_hex() {
    assert_eq!(parse_int_arg("0xff"), (255, false));
}

#[test]
fn test_parse_int_octal() {
    assert_eq!(parse_int_arg("077"), (63, false));
}

#[test]
fn test_parse_int_char() {
    assert_eq!(parse_int_arg("'A"), (65, false));
}

#[test]
fn test_parse_int_empty() {
    assert_eq!(parse_int_arg(""), (0, false));
}
