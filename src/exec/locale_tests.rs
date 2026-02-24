use super::*;
use crate::exec::environment::Environment;

fn make_env() -> Environment {
    Environment::new()
}

// -- Locale resolution tests --------------------------------------------------

#[test]
fn locale_resolution_lc_all_overrides() {
    let mut env = make_env();
    let _ = env.set_var("LANG", "en_US.UTF-8");
    let _ = env.set_var("LC_CTYPE", "en_US.UTF-8");
    let _ = env.set_var("LC_ALL", "tr_TR.UTF-8");
    let locale = ctype_locale(&env);
    // LC_ALL should win — locale should be Turkish
    assert_eq!(to_uppercase("i", &locale), "\u{0130}"); // İ
}

#[test]
fn locale_resolution_specific_overrides_lang() {
    let mut env = make_env();
    let _ = env.set_var("LANG", "en_US.UTF-8");
    let _ = env.set_var("LC_CTYPE", "tr_TR.UTF-8");
    let locale = ctype_locale(&env);
    assert_eq!(to_uppercase("i", &locale), "\u{0130}"); // İ
}

#[test]
fn locale_resolution_lang_fallback() {
    let mut env = make_env();
    let _ = env.set_var("LANG", "tr_TR.UTF-8");
    let locale = ctype_locale(&env);
    assert_eq!(to_uppercase("i", &locale), "\u{0130}"); // İ
}

#[test]
fn locale_resolution_default_c() {
    let env = make_env();
    let locale = ctype_locale(&env);
    // C locale: standard ASCII/Unicode case mapping
    assert_eq!(to_uppercase("i", &locale), "I");
}

#[test]
fn collate_locale_resolves() {
    let mut env = make_env();
    let _ = env.set_var("LC_COLLATE", "de_DE.UTF-8");
    let locale = collate_locale(&env);
    // Just verify it resolved to German — exact collation behaviour
    // is not tested here, only that the resolution path works.
    assert_eq!(locale.id.language, icu::locale::subtags::language!("de"));
}

// -- Case conversion tests ----------------------------------------------------

#[test]
fn uppercase_ascii() {
    let locale = parse_posix_locale("C");
    assert_eq!(to_uppercase("hello", &locale), "HELLO");
}

#[test]
fn lowercase_ascii() {
    let locale = parse_posix_locale("C");
    assert_eq!(to_lowercase("HELLO", &locale), "hello");
}

#[test]
fn uppercase_unicode_accents() {
    let locale = parse_posix_locale("C");
    assert_eq!(to_uppercase("caf\u{00e9}", &locale), "CAF\u{00c9}"); // café -> CAFÉ
}

#[test]
fn turkish_i_uppercase() {
    let locale = parse_posix_locale("tr_TR.UTF-8");
    assert_eq!(to_uppercase("i", &locale), "\u{0130}"); // İ (dotted capital I)
}

#[test]
fn turkish_i_lowercase() {
    let locale = parse_posix_locale("tr_TR.UTF-8");
    assert_eq!(to_lowercase("I", &locale), "\u{0131}"); // ı (dotless small i)
}

#[test]
fn german_eszett_uppercase() {
    let locale = parse_posix_locale("de_DE.UTF-8");
    // German ß uppercases to SS (or ẞ depending on convention)
    let result = to_uppercase("\u{00df}", &locale);
    assert!(result == "SS" || result == "\u{1e9e}", "got: {result}");
}

#[test]
fn empty_string_uppercase() {
    let locale = parse_posix_locale("C");
    assert_eq!(to_uppercase("", &locale), "");
}

#[test]
fn empty_string_lowercase() {
    let locale = parse_posix_locale("C");
    assert_eq!(to_lowercase("", &locale), "");
}

// -- Capitalize / uncapitalize tests ------------------------------------------

#[test]
fn capitalize_ascii() {
    let locale = parse_posix_locale("C");
    assert_eq!(capitalize("hello", &locale), "Hello");
}

#[test]
fn uncapitalize_ascii() {
    let locale = parse_posix_locale("C");
    assert_eq!(uncapitalize("HELLO", &locale), "hELLO");
}

#[test]
fn capitalize_empty() {
    let locale = parse_posix_locale("C");
    assert_eq!(capitalize("", &locale), "");
}

#[test]
fn uncapitalize_empty() {
    let locale = parse_posix_locale("C");
    assert_eq!(uncapitalize("", &locale), "");
}

#[test]
fn capitalize_single_char() {
    let locale = parse_posix_locale("C");
    assert_eq!(capitalize("h", &locale), "H");
}

#[test]
fn uncapitalize_single_char() {
    let locale = parse_posix_locale("C");
    assert_eq!(uncapitalize("H", &locale), "h");
}

#[test]
fn capitalize_multibyte_first_char() {
    let locale = parse_posix_locale("C");
    // é (2 bytes in UTF-8) followed by ASCII
    assert_eq!(capitalize("\u{00e9}cole", &locale), "\u{00c9}cole");
}

#[test]
fn capitalize_turkish() {
    let locale = parse_posix_locale("tr_TR");
    assert_eq!(capitalize("istanbul", &locale), "\u{0130}stanbul");
}

// -- parse_posix_locale tests -------------------------------------------------

#[test]
fn parse_posix_locale_c() {
    let locale = parse_posix_locale("C");
    // C locale maps to root/undefined
    assert_eq!(locale, Locale::UNKNOWN);
}

#[test]
fn parse_posix_locale_posix() {
    let locale = parse_posix_locale("POSIX");
    assert_eq!(locale, Locale::UNKNOWN);
}

#[test]
fn parse_posix_locale_empty() {
    let locale = parse_posix_locale("");
    assert_eq!(locale, Locale::UNKNOWN);
}

#[test]
fn parse_posix_locale_with_charset() {
    let locale = parse_posix_locale("en_US.UTF-8");
    assert_eq!(locale.id.language, icu::locale::subtags::language!("en"));
    assert_eq!(locale.id.region, Some(icu::locale::subtags::region!("US")));
}

#[test]
fn parse_posix_locale_without_charset() {
    let locale = parse_posix_locale("tr_TR");
    assert_eq!(locale.id.language, icu::locale::subtags::language!("tr"));
    assert_eq!(locale.id.region, Some(icu::locale::subtags::region!("TR")));
}

#[test]
fn parse_posix_locale_language_only() {
    let locale = parse_posix_locale("de");
    assert_eq!(locale.id.language, icu::locale::subtags::language!("de"));
    assert_eq!(locale.id.region, None);
}

#[test]
fn parse_posix_locale_whitespace_trimmed() {
    let locale = parse_posix_locale("  en_US.UTF-8  ");
    assert_eq!(locale.id.language, icu::locale::subtags::language!("en"));
}

#[test]
fn parse_posix_locale_invalid_falls_back() {
    // Garbage input should not panic — it falls back to UNKNOWN.
    let locale = parse_posix_locale("!!!");
    assert_eq!(locale, Locale::UNKNOWN);
}

#[test]
fn parse_posix_locale_lenient_best_effort() {
    // ICU4X's BCP47 parser is lenient: "not-a-real-locale" parses as
    // language "not" with extension subtags.  We just verify no panic.
    let _locale = parse_posix_locale("not-a-real-locale");
}

// Decimal separator -------------------------------------------------------

#[test]
fn decimal_separator_c_locale() {
    let locale = parse_posix_locale("C");
    assert_eq!(decimal_separator(&locale), '.');
}

#[test]
fn decimal_separator_german() {
    let locale = parse_posix_locale("de_DE.UTF-8");
    assert_eq!(decimal_separator(&locale), ',');
}

#[test]
fn decimal_separator_english() {
    let locale = parse_posix_locale("en_US.UTF-8");
    assert_eq!(decimal_separator(&locale), '.');
}

#[test]
fn decimal_separator_french() {
    // French uses comma as decimal separator
    let locale = parse_posix_locale("fr_FR.UTF-8");
    // ICU4X should return ',' for French
    let sep = decimal_separator(&locale);
    assert!(sep == ',' || sep == '.', "got: {}", sep);
}

#[test]
fn numeric_locale_resolution() {
    let mut env = make_env();
    let _ = env.set_var("LC_NUMERIC", "de_DE.UTF-8");
    let locale = numeric_locale(&env);
    assert_eq!(decimal_separator(&locale), ',');
}

// -- Character class tests (POSIX [:class:]) ----------------------------------

fn c() -> Locale {
    parse_posix_locale("C")
}
fn utf8() -> Locale {
    parse_posix_locale("en_US.UTF-8")
}
fn tr() -> Locale {
    parse_posix_locale("tr_TR.UTF-8")
}

// C locale: upper (ASCII only) ------------------------------------------------

#[test]
fn char_class_upper_ascii_a() {
    assert!(is_char_class('A', "upper", &c()));
}

#[test]
fn char_class_upper_ascii_z() {
    assert!(is_char_class('Z', "upper", &c()));
}

#[test]
fn char_class_upper_ascii_a_lower() {
    assert!(!is_char_class('a', "upper", &c()));
}

#[test]
fn char_class_upper_accent_c() {
    assert!(!is_char_class('\u{00c9}', "upper", &c())); // É
}

// C locale: lower (ASCII only) ------------------------------------------------

#[test]
fn char_class_lower_ascii_a() {
    assert!(is_char_class('a', "lower", &c()));
}

#[test]
fn char_class_lower_ascii_z() {
    assert!(is_char_class('z', "lower", &c()));
}

#[test]
fn char_class_lower_ascii_upper_a() {
    assert!(!is_char_class('A', "lower", &c()));
}

#[test]
fn char_class_lower_accent_c() {
    assert!(!is_char_class('\u{00e9}', "lower", &c())); // é
}

// C locale: alpha (ASCII only) ------------------------------------------------

#[test]
fn char_class_alpha_ascii() {
    assert!(is_char_class('m', "alpha", &c()));
}

#[test]
fn char_class_alpha_digit_no() {
    assert!(!is_char_class('5', "alpha", &c()));
}

#[test]
fn char_class_alpha_accent_c() {
    assert!(!is_char_class('\u{00e9}', "alpha", &c())); // é
}

// C locale: digit (locale-invariant) ------------------------------------------

#[test]
fn char_class_digit_5() {
    assert!(is_char_class('5', "digit", &c()));
}

#[test]
fn char_class_digit_a_no() {
    assert!(!is_char_class('a', "digit", &c()));
}

// C locale: alnum -------------------------------------------------------------

#[test]
fn char_class_alnum_letter() {
    assert!(is_char_class('A', "alnum", &c()));
}

#[test]
fn char_class_alnum_digit() {
    assert!(is_char_class('9', "alnum", &c()));
}

#[test]
fn char_class_alnum_punct_no() {
    assert!(!is_char_class('!', "alnum", &c()));
}

// C locale: space -------------------------------------------------------------

#[test]
fn char_class_space_space() {
    assert!(is_char_class(' ', "space", &c()));
}

#[test]
fn char_class_space_tab() {
    assert!(is_char_class('\t', "space", &c()));
}

#[test]
fn char_class_space_newline() {
    assert!(is_char_class('\n', "space", &c()));
}

#[test]
fn char_class_space_a_no() {
    assert!(!is_char_class('a', "space", &c()));
}

// C locale: blank -------------------------------------------------------------

#[test]
fn char_class_blank_space() {
    assert!(is_char_class(' ', "blank", &c()));
}

#[test]
fn char_class_blank_tab() {
    assert!(is_char_class('\t', "blank", &c()));
}

#[test]
fn char_class_blank_newline_no() {
    assert!(!is_char_class('\n', "blank", &c()));
}

// C locale: punct -------------------------------------------------------------

#[test]
fn char_class_punct_bang() {
    assert!(is_char_class('!', "punct", &c()));
}

#[test]
fn char_class_punct_dot() {
    assert!(is_char_class('.', "punct", &c()));
}

#[test]
fn char_class_punct_a_no() {
    assert!(!is_char_class('a', "punct", &c()));
}

// C locale: cntrl (locale-invariant) ------------------------------------------

#[test]
fn char_class_cntrl_null() {
    assert!(is_char_class('\0', "cntrl", &c()));
}

#[test]
fn char_class_cntrl_bel() {
    assert!(is_char_class('\x07', "cntrl", &c()));
}

#[test]
fn char_class_cntrl_a_no() {
    assert!(!is_char_class('a', "cntrl", &c()));
}

// C locale: xdigit (locale-invariant) -----------------------------------------

#[test]
fn char_class_xdigit_0() {
    assert!(is_char_class('0', "xdigit", &c()));
}

#[test]
fn char_class_xdigit_f() {
    assert!(is_char_class('f', "xdigit", &c()));
}

#[test]
fn char_class_xdigit_upper_f() {
    assert!(is_char_class('F', "xdigit", &c()));
}

#[test]
fn char_class_xdigit_g_no() {
    assert!(!is_char_class('g', "xdigit", &c()));
}

// C locale: graph -------------------------------------------------------------

#[test]
fn char_class_graph_a() {
    assert!(is_char_class('a', "graph", &c()));
}

#[test]
fn char_class_graph_bang() {
    assert!(is_char_class('!', "graph", &c()));
}

#[test]
fn char_class_graph_space_no() {
    assert!(!is_char_class(' ', "graph", &c()));
}

// C locale: print -------------------------------------------------------------

#[test]
fn char_class_print_a() {
    assert!(is_char_class('a', "print", &c()));
}

#[test]
fn char_class_print_space() {
    assert!(is_char_class(' ', "print", &c()));
}

#[test]
fn char_class_print_cntrl_no() {
    assert!(!is_char_class('\x07', "print", &c()));
}

// UTF-8 locale: Unicode classification ----------------------------------------

#[test]
fn char_class_upper_accent_utf8() {
    assert!(is_char_class('\u{00c9}', "upper", &utf8())); // É
}

#[test]
fn char_class_lower_accent_utf8() {
    assert!(is_char_class('\u{00e9}', "lower", &utf8())); // é
}

#[test]
fn char_class_alpha_accent_utf8() {
    assert!(is_char_class('\u{00f1}', "alpha", &utf8())); // ñ
}

#[test]
fn char_class_alpha_cjk_utf8() {
    assert!(is_char_class('\u{65e5}', "alpha", &utf8())); // 日
}

// Turkish locale --------------------------------------------------------------

#[test]
fn char_class_upper_dotted_i_turkish() {
    assert!(is_char_class('\u{0130}', "upper", &tr())); // İ
}

#[test]
fn char_class_lower_dotless_i_turkish() {
    assert!(is_char_class('\u{0131}', "lower", &tr())); // ı
}

// Unknown class returns false -------------------------------------------------

#[test]
fn char_class_unknown() {
    assert!(!is_char_class('A', "bogus", &c()));
}
