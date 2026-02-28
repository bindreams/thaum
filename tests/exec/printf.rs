use crate::*;

// printf builtin ------------------------------------------------------------------------------------------------------

#[testutil::test]
fn printf_basic_string() {
    let (out, _) = exec_ok("printf '%s\\n' hello");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn printf_basic_integer() {
    let (out, _) = exec_ok("printf '%d\\n' 42");
    assert_eq!(out, "42\n");
}

#[testutil::test]
fn printf_hex() {
    let (out, _) = exec_ok("printf '%x\\n' 255");
    assert_eq!(out, "ff\n");
}

#[testutil::test]
fn printf_hex_upper() {
    let (out, _) = exec_ok("printf '%X\\n' 255");
    assert_eq!(out, "FF\n");
}

#[testutil::test]
fn printf_octal() {
    let (out, _) = exec_ok("printf '%o\\n' 8");
    assert_eq!(out, "10\n");
}

#[testutil::test]
fn printf_unsigned() {
    let (out, _) = exec_ok("printf '%u\\n' 42");
    assert_eq!(out, "42\n");
}

#[testutil::test]
fn printf_width_string() {
    let (out, _) = exec_ok("printf '[%10s]\\n' hi");
    assert_eq!(out, "[        hi]\n");
}

#[testutil::test]
fn printf_left_align() {
    let (out, _) = exec_ok("printf '[%-10s]\\n' hi");
    assert_eq!(out, "[hi        ]\n");
}

#[testutil::test]
fn printf_zero_pad() {
    let (out, _) = exec_ok("printf '[%05d]\\n' 42");
    assert_eq!(out, "[00042]\n");
}

#[testutil::test]
fn printf_precision_string() {
    let (out, _) = exec_ok("printf '[%.3s]\\n' hello");
    assert_eq!(out, "[hel]\n");
}

#[testutil::test]
fn printf_precision_integer() {
    let (out, _) = exec_ok("printf '[%6.4d]\\n' 42");
    assert_eq!(out, "[  0042]\n");
}

#[testutil::test]
fn printf_float() {
    let (out, _) = exec_ok("printf '[%.2f]\\n' 3.14159");
    assert_eq!(out, "[3.14]\n");
}

#[testutil::test]
fn printf_escape_newline() {
    let (out, _) = exec_ok("printf 'a\\nb\\n'");
    assert_eq!(out, "a\nb\n");
}

#[testutil::test]
fn printf_escape_tab() {
    let (out, _) = exec_ok("printf 'a\\tb\\n'");
    assert_eq!(out, "a\tb\n");
}

#[testutil::test]
fn printf_escape_hex() {
    let (out, _) = exec_ok("printf '\\x41\\n'");
    assert_eq!(out, "A\n");
}

#[testutil::test]
fn printf_percent_literal() {
    let (out, _) = exec_ok("printf '%%\\n'");
    assert_eq!(out, "%\n");
}

#[testutil::test]
fn printf_missing_arg_string() {
    let (out, _) = exec_ok("printf '%s|%s\\n' hello");
    assert_eq!(out, "hello|\n");
}

#[testutil::test]
fn printf_missing_arg_int() {
    let (out, _) = exec_ok("printf '%d\\n'");
    assert_eq!(out, "0\n");
}

#[testutil::test]
fn printf_cyclic_args() {
    let (out, _) = exec_ok("printf '%s\\n' a b c");
    assert_eq!(out, "a\nb\nc\n");
}

#[testutil::test]
fn printf_var() {
    let (out, _) = exec_ok("printf -v x '%d' 42; echo $x");
    assert_eq!(out, "42\n");
}

#[testutil::test]
fn printf_shell_quote() {
    let (out, _) = exec_ok("printf '%q\\n' 'hello world'");
    // Should contain some form of quoting
    assert!(out.contains("hello") && out.contains("world"));
    assert!(out.trim() != "hello world"); // must be quoted somehow
}

#[testutil::test]
fn printf_backslash_b() {
    let (out, _) = exec_ok("printf '%b\\n' 'a\\nb'");
    assert_eq!(out, "a\nb\n");
}

#[testutil::test]
fn printf_no_trailing_newline() {
    let (out, _) = exec_ok("printf '%s' hello");
    assert_eq!(out, "hello");
}

#[testutil::test]
fn printf_hex_arg() {
    let (out, _) = exec_ok("printf '%d\\n' 0xff");
    assert_eq!(out, "255\n");
}

#[testutil::test]
fn printf_octal_arg() {
    let (out, _) = exec_ok("printf '%d\\n' 077");
    assert_eq!(out, "63\n");
}

#[testutil::test]
fn printf_char_arg() {
    let (out, _) = exec_ok("printf '%d\\n' \"'A\"");
    assert_eq!(out, "65\n");
}

#[testutil::test]
fn printf_hash_hex() {
    let (out, _) = exec_ok("printf '%#x\\n' 255");
    assert_eq!(out, "0xff\n");
}

#[testutil::test]
fn printf_hash_octal() {
    let (out, _) = exec_ok("printf '%#o\\n' 8");
    assert_eq!(out, "010\n");
}

#[testutil::test]
fn printf_char_conversion() {
    let (out, _) = exec_ok("printf '%c\\n' hello");
    assert_eq!(out, "h\n");
}

#[testutil::test]
fn printf_negative_zero_pad() {
    let (out, _) = exec_ok("printf '[%010d]\\n' -42");
    assert_eq!(out, "[-000000042]\n");
}

#[testutil::test]
fn printf_strftime_epoch() {
    // Epoch 0 in UTC is 1970
    let (out, _) = exec_ok("TZ=UTC printf '%(%Y)T\\n' 0");
    assert_eq!(out, "1970\n");
}

#[testutil::test]
fn printf_strftime_current() {
    let (out, _) = exec_ok("printf '%(%Y)T\\n' -1");
    let year: i32 = out.trim().parse().unwrap();
    assert!((2024..=2030).contains(&year));
}

// printf LC_TIME strftime ---------------------------------------------------------------------------------------------

#[testutil::test]
fn printf_strftime_weekday_german() {
    // 2001-09-09 is a Sunday in UTC — "Sonntag" in German
    let (out, _) = exec_ok("TZ=UTC LC_TIME=de_DE.UTF-8 printf '%(%A)T' 1000000000");
    assert_eq!(out, "Sonntag");
}

#[testutil::test]
fn printf_strftime_month_french() {
    // 2001-09-09 — September in French is "septembre"
    let (out, _) = exec_ok("TZ=UTC LC_TIME=fr_FR.UTF-8 printf '%(%B)T' 1000000000");
    assert_eq!(out, "septembre");
}

#[testutil::test]
fn printf_strftime_lc_time_overrides_lang() {
    // LC_TIME should override LANG for strftime
    let (out, _) = exec_ok("TZ=UTC LANG=en_US.UTF-8 LC_TIME=de_DE.UTF-8 printf '%(%A)T' 1000000000");
    assert_eq!(out, "Sonntag");
}

#[testutil::test]
fn printf_strftime_c_locale_english() {
    // C locale should give English weekday names
    let (out, _) = exec_ok("TZ=UTC LC_TIME=C printf '%(%A)T' 1000000000");
    assert_eq!(out, "Sunday");
}

#[testutil::test]
fn printf_strftime_mixed_locale_and_numeric_codes() {
    // Mix locale-sensitive and numeric codes in the same format string
    let (out, _) = exec_ok("TZ=UTC LC_TIME=de_DE.UTF-8 printf '%(%A %Y-%m-%d)T' 1000000000");
    assert_eq!(out, "Sonntag 2001-09-09");
}

// printf LC_NUMERIC ---------------------------------------------------------------------------------------------------

#[testutil::test]
fn printf_lc_numeric_output() {
    // German locale: decimal separator is comma. Integer arg avoids input ambiguity.
    let (out, _) = exec_ok("LC_NUMERIC=de_DE.UTF-8 printf '%.1f\\n' 3");
    assert_eq!(out, "3,0\n");
}

#[testutil::test]
fn printf_lc_numeric_input_comma() {
    // In German locale, "3,14" is a valid float (comma is decimal sep).
    let (out, _) = exec_ok("LC_NUMERIC=de_DE.UTF-8 printf '%.2f\\n' '3,14'");
    assert_eq!(out, "3,14\n");
}

#[testutil::test]
fn printf_lc_numeric_c_locale() {
    // C locale uses '.' — default behaviour should be unchanged.
    let (out, _) = exec_ok("LC_NUMERIC=C printf '%.2f\\n' 3.14");
    assert_eq!(out, "3.14\n");
}
