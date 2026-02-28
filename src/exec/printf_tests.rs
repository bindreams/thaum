use super::*;
use crate::exec::environment::Environment;

testutil::default_labels!(exec);

fn fmt(format: &str, args: &[&str]) -> String {
    fmt_with_sep(format, args, '.')
}

fn fmt_with_sep(format: &str, args: &[&str], decimal_sep: char) -> String {
    let env = Environment::new();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut buf = Vec::new();
    printf_format(format, &args, &mut buf, decimal_sep, &env);
    String::from_utf8(buf).unwrap()
}

#[testutil::test]
fn test_basic_string() {
    assert_eq!(fmt("%s", &["hello"]), "hello");
}

#[testutil::test]
fn test_basic_int() {
    assert_eq!(fmt("%d", &["42"]), "42");
}

#[testutil::test]
fn test_hex() {
    assert_eq!(fmt("%x", &["255"]), "ff");
    assert_eq!(fmt("%X", &["255"]), "FF");
}

#[testutil::test]
fn test_octal() {
    assert_eq!(fmt("%o", &["8"]), "10");
}

#[testutil::test]
fn test_escape_newline() {
    assert_eq!(fmt("a\\nb", &[]), "a\nb");
}

#[testutil::test]
fn test_escape_hex() {
    assert_eq!(fmt("\\x41", &[]), "A");
}

#[testutil::test]
fn test_cyclic() {
    assert_eq!(fmt("%s\\n", &["a", "b", "c"]), "a\nb\nc\n");
}

#[testutil::test]
fn test_missing_arg_string() {
    assert_eq!(fmt("%s|%s", &["hello"]), "hello|");
}

#[testutil::test]
fn test_missing_arg_int() {
    assert_eq!(fmt("%d", &[]), "0");
}

#[testutil::test]
fn test_width_string() {
    assert_eq!(fmt("[%10s]", &["hi"]), "[        hi]");
}

#[testutil::test]
fn test_left_align() {
    assert_eq!(fmt("[%-10s]", &["hi"]), "[hi        ]");
}

#[testutil::test]
fn test_zero_pad() {
    assert_eq!(fmt("[%05d]", &["42"]), "[00042]");
}

#[testutil::test]
fn test_precision_string() {
    assert_eq!(fmt("[%.3s]", &["hello"]), "[hel]");
}

#[testutil::test]
fn test_precision_int() {
    assert_eq!(fmt("[%6.4d]", &["42"]), "[  0042]");
}

#[testutil::test]
fn test_float() {
    assert_eq!(fmt("[%.2f]", &["3.14159"]), "[3.14]");
}

#[testutil::test]
fn test_percent_literal() {
    assert_eq!(fmt("%%", &[]), "%");
}

#[testutil::test]
fn test_hex_arg() {
    assert_eq!(fmt("%d", &["0xff"]), "255");
}

#[testutil::test]
fn test_octal_arg() {
    assert_eq!(fmt("%d", &["077"]), "63");
}

#[testutil::test]
fn test_char_arg() {
    assert_eq!(fmt("%d", &["'A"]), "65");
}

#[testutil::test]
fn test_hash_hex() {
    assert_eq!(fmt("%#x", &["255"]), "0xff");
}

#[testutil::test]
fn test_hash_octal() {
    assert_eq!(fmt("%#o", &["8"]), "010");
}

#[testutil::test]
fn test_char_conv() {
    assert_eq!(fmt("%c", &["hello"]), "h");
}

#[testutil::test]
fn test_negative_zero_pad() {
    assert_eq!(fmt("[%010d]", &["-42"]), "[-000000042]");
}

#[testutil::test]
fn test_shell_quote_safe() {
    assert_eq!(fmt("%q", &["hello"]), "hello");
}

#[testutil::test]
fn test_shell_quote_special() {
    let result = fmt("%q", &["hello world"]);
    assert!(result.contains("hello") && result.contains("world"));
    assert_ne!(result, "hello world");
}

#[testutil::test]
fn test_backslash_b() {
    assert_eq!(fmt("%b", &["a\\nb"]), "a\nb");
}

#[testutil::test]
fn test_unsigned() {
    assert_eq!(fmt("%u", &["42"]), "42");
}

#[testutil::test]
fn test_parse_int_hex() {
    assert_eq!(parse_int_arg("0xff"), (255, false));
}

#[testutil::test]
fn test_parse_int_octal() {
    assert_eq!(parse_int_arg("077"), (63, false));
}

#[testutil::test]
fn test_parse_int_char() {
    assert_eq!(parse_int_arg("'A"), (65, false));
}

#[testutil::test]
fn test_parse_int_empty() {
    assert_eq!(parse_int_arg(""), (0, false));
}

// LC_NUMERIC decimal separator tests ==========================================

#[testutil::test]
fn test_float_output_comma_separator() {
    // With comma as decimal separator, output should use comma
    assert_eq!(fmt_with_sep("%.2f", &["3.14"], ','), "3,14");
}

#[testutil::test]
fn test_float_input_comma_separator() {
    // With comma as decimal separator, input "3,14" should parse correctly
    assert_eq!(fmt_with_sep("%.2f", &["3,14"], ','), "3,14");
}

#[testutil::test]
fn test_float_dot_input_rejected_with_comma_locale() {
    // In a comma-locale, "3.14" has a dot which is not the locale separator,
    // so it gets passed through as-is (dot is not replaced). Rust's f64 parser
    // accepts dot, so "3.14" still parses as 3.14 but the output uses comma.
    assert_eq!(fmt_with_sep("%.2f", &["3.14"], ','), "3,14");
}

#[testutil::test]
fn test_float_integer_arg_comma_locale() {
    // Integer argument with comma locale: output uses comma
    assert_eq!(fmt_with_sep("%.2f", &["3"], ','), "3,00");
}

#[testutil::test]
fn test_scientific_comma_separator() {
    // %e with comma decimal separator
    assert_eq!(fmt_with_sep("%.2e", &["1234.5"], ','), "1,23e+03");
}

#[testutil::test]
fn test_general_comma_separator() {
    // %g with comma decimal separator
    assert_eq!(fmt_with_sep("%g", &["3.14"], ','), "3,14");
}

#[testutil::test]
fn test_dot_locale_unchanged() {
    // With dot as separator (default), behaviour is unchanged
    assert_eq!(fmt_with_sep("%.2f", &["3.14"], '.'), "3.14");
}
