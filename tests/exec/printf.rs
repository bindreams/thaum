// printf builtin ------------------------------------------------------------------------------------------------------

#[skuld::test]
fn printf_basic_string() {
    let r = exec!("printf '%s\\n' hello");
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn printf_basic_integer() {
    let r = exec!("printf '%d\\n' 42");
    assert_eq!(r.stdout(), "42\n");
}

#[skuld::test]
fn printf_hex() {
    let r = exec!("printf '%x\\n' 255");
    assert_eq!(r.stdout(), "ff\n");
}

#[skuld::test]
fn printf_hex_upper() {
    let r = exec!("printf '%X\\n' 255");
    assert_eq!(r.stdout(), "FF\n");
}

#[skuld::test]
fn printf_octal() {
    let r = exec!("printf '%o\\n' 8");
    assert_eq!(r.stdout(), "10\n");
}

#[skuld::test]
fn printf_unsigned() {
    let r = exec!("printf '%u\\n' 42");
    assert_eq!(r.stdout(), "42\n");
}

#[skuld::test]
fn printf_width_string() {
    let r = exec!("printf '[%10s]\\n' hi");
    assert_eq!(r.stdout(), "[        hi]\n");
}

#[skuld::test]
fn printf_left_align() {
    let r = exec!("printf '[%-10s]\\n' hi");
    assert_eq!(r.stdout(), "[hi        ]\n");
}

#[skuld::test]
fn printf_zero_pad() {
    let r = exec!("printf '[%05d]\\n' 42");
    assert_eq!(r.stdout(), "[00042]\n");
}

#[skuld::test]
fn printf_precision_string() {
    let r = exec!("printf '[%.3s]\\n' hello");
    assert_eq!(r.stdout(), "[hel]\n");
}

#[skuld::test]
fn printf_precision_integer() {
    let r = exec!("printf '[%6.4d]\\n' 42");
    assert_eq!(r.stdout(), "[  0042]\n");
}

#[skuld::test]
fn printf_float() {
    let r = exec!("printf '[%.2f]\\n' 3.14159");
    assert_eq!(r.stdout(), "[3.14]\n");
}

#[skuld::test]
fn printf_escape_newline() {
    let r = exec!("printf 'a\\nb\\n'");
    assert_eq!(r.stdout(), "a\nb\n");
}

#[skuld::test]
fn printf_escape_tab() {
    let r = exec!("printf 'a\\tb\\n'");
    assert_eq!(r.stdout(), "a\tb\n");
}

#[skuld::test]
fn printf_escape_hex() {
    let r = exec!("printf '\\x41\\n'");
    assert_eq!(r.stdout(), "A\n");
}

#[skuld::test]
fn printf_percent_literal() {
    let r = exec!("printf '%%\\n'");
    assert_eq!(r.stdout(), "%\n");
}

#[skuld::test]
fn printf_missing_arg_string() {
    let r = exec!("printf '%s|%s\\n' hello");
    assert_eq!(r.stdout(), "hello|\n");
}

#[skuld::test]
fn printf_missing_arg_int() {
    let r = exec!("printf '%d\\n'");
    assert_eq!(r.stdout(), "0\n");
}

#[skuld::test]
fn printf_cyclic_args() {
    let r = exec!("printf '%s\\n' a b c");
    assert_eq!(r.stdout(), "a\nb\nc\n");
}

#[skuld::test]
fn printf_var() {
    let r = exec!("printf -v x '%d' 42; echo $x");
    assert_eq!(r.stdout(), "42\n");
}

#[skuld::test]
fn printf_shell_quote() {
    let r = exec!("printf '%q\\n' 'hello world'");
    // Should contain some form of quoting
    assert!(r.stdout().contains("hello") && r.stdout().contains("world"));
    assert!(r.stdout().trim() != "hello world"); // must be quoted somehow
}

#[skuld::test]
fn printf_backslash_b() {
    let r = exec!("printf '%b\\n' 'a\\nb'");
    assert_eq!(r.stdout(), "a\nb\n");
}

#[skuld::test]
fn printf_no_trailing_newline() {
    let r = exec!("printf '%s' hello");
    assert_eq!(r.stdout(), "hello");
}

#[skuld::test]
fn printf_hex_arg() {
    let r = exec!("printf '%d\\n' 0xff");
    assert_eq!(r.stdout(), "255\n");
}

#[skuld::test]
fn printf_octal_arg() {
    let r = exec!("printf '%d\\n' 077");
    assert_eq!(r.stdout(), "63\n");
}

#[skuld::test]
fn printf_char_arg() {
    let r = exec!("printf '%d\\n' \"'A\"");
    assert_eq!(r.stdout(), "65\n");
}

#[skuld::test]
fn printf_hash_hex() {
    let r = exec!("printf '%#x\\n' 255");
    assert_eq!(r.stdout(), "0xff\n");
}

#[skuld::test]
fn printf_hash_octal() {
    let r = exec!("printf '%#o\\n' 8");
    assert_eq!(r.stdout(), "010\n");
}

#[skuld::test]
fn printf_char_conversion() {
    let r = exec!("printf '%c\\n' hello");
    assert_eq!(r.stdout(), "h\n");
}

#[skuld::test]
fn printf_negative_zero_pad() {
    let r = exec!("printf '[%010d]\\n' -42");
    assert_eq!(r.stdout(), "[-000000042]\n");
}

#[skuld::test]
fn printf_strftime_epoch() {
    // Epoch 0 in UTC is 1970
    let r = exec!("TZ=UTC printf '%(%Y)T\\n' 0");
    assert_eq!(r.stdout(), "1970\n");
}

#[skuld::test]
fn printf_strftime_current() {
    let r = exec!("printf '%(%Y)T\\n' -1");
    let year: i32 = r.stdout().trim().parse().unwrap();
    assert!((2024..=2030).contains(&year));
}

// printf LC_TIME strftime ---------------------------------------------------------------------------------------------

#[skuld::test]
fn printf_strftime_weekday_german() {
    // 2001-09-09 is a Sunday in UTC — "Sonntag" in German
    let r = exec!("TZ=UTC LC_TIME=de_DE.UTF-8 printf '%(%A)T' 1000000000");
    assert_eq!(r.stdout(), "Sonntag");
}

#[skuld::test]
fn printf_strftime_month_french() {
    // 2001-09-09 — September in French is "septembre"
    let r = exec!("TZ=UTC LC_TIME=fr_FR.UTF-8 printf '%(%B)T' 1000000000");
    assert_eq!(r.stdout(), "septembre");
}

#[skuld::test]
fn printf_strftime_lc_time_overrides_lang() {
    // LC_TIME should override LANG for strftime
    let r = exec!("TZ=UTC LANG=en_US.UTF-8 LC_TIME=de_DE.UTF-8 printf '%(%A)T' 1000000000");
    assert_eq!(r.stdout(), "Sonntag");
}

#[skuld::test]
fn printf_strftime_c_locale_english() {
    // C locale should give English weekday names
    let r = exec!("TZ=UTC LC_TIME=C printf '%(%A)T' 1000000000");
    assert_eq!(r.stdout(), "Sunday");
}

#[skuld::test]
fn printf_strftime_mixed_locale_and_numeric_codes() {
    // Mix locale-sensitive and numeric codes in the same format string
    let r = exec!("TZ=UTC LC_TIME=de_DE.UTF-8 printf '%(%A %Y-%m-%d)T' 1000000000");
    assert_eq!(r.stdout(), "Sonntag 2001-09-09");
}

// printf LC_NUMERIC ---------------------------------------------------------------------------------------------------

#[skuld::test]
fn printf_lc_numeric_output() {
    // German locale: decimal separator is comma. Integer arg avoids input ambiguity.
    let r = exec!("LC_NUMERIC=de_DE.UTF-8 printf '%.1f\\n' 3");
    assert_eq!(r.stdout(), "3,0\n");
}

#[skuld::test]
fn printf_lc_numeric_input_comma() {
    // In German locale, "3,14" is a valid float (comma is decimal sep).
    let r = exec!("LC_NUMERIC=de_DE.UTF-8 printf '%.2f\\n' '3,14'");
    assert_eq!(r.stdout(), "3,14\n");
}

#[skuld::test]
fn printf_lc_numeric_c_locale() {
    // C locale uses '.' — default behaviour should be unchanged.
    let r = exec!("LC_NUMERIC=C printf '%.2f\\n' 3.14");
    assert_eq!(r.stdout(), "3.14\n");
}
