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
