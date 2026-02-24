//! Locale-aware string operations via ICU4X.
//!
//! Shells respect `LC_ALL`, `LC_CTYPE`, `LC_COLLATE`, and `LANG` environment
//! variables for case conversion, collation, and character classification.
//! Resolution order per POSIX: `LC_ALL` > specific `LC_*` > `LANG` > `"C"`.

use icu::casemap::CaseMapper;
use icu::locale::Locale;

use crate::exec::environment::Environment;

/// Resolve a locale for the given POSIX category from the shell environment.
///
/// POSIX resolution: LC_ALL overrides everything, then the specific LC_*
/// variable (e.g., LC_CTYPE), then LANG, then "C" as the ultimate fallback.
pub fn resolve_locale(env: &Environment, category: &str) -> Locale {
    let posix_str = env
        .get_var("LC_ALL")
        .or_else(|| env.get_var(category))
        .or_else(|| env.get_var("LANG"))
        .unwrap_or("C");
    parse_posix_locale(posix_str)
}

/// Resolve LC_CTYPE (case conversion, character classification).
pub fn ctype_locale(env: &Environment) -> Locale {
    resolve_locale(env, "LC_CTYPE")
}

/// Resolve LC_COLLATE (string comparison ordering).
pub fn collate_locale(env: &Environment) -> Locale {
    resolve_locale(env, "LC_COLLATE")
}

/// Compare two strings using locale-aware collation (for `[[ a < b ]]`).
pub fn compare_strings(a: &str, b: &str, locale: &Locale) -> std::cmp::Ordering {
    use icu::collator::{Collator, CollatorPreferences};
    let prefs = CollatorPreferences::from(&locale.id);
    match Collator::try_new(prefs, Default::default()) {
        Ok(collator) => collator.compare(a, b),
        Err(_) => a.cmp(b), // fallback to byte order
    }
}

/// Uppercase an entire string respecting locale (like bash `${var^^}`).
pub fn to_uppercase(s: &str, locale: &Locale) -> String {
    let cm = CaseMapper::new();
    cm.uppercase_to_string(s, &locale.id).into_owned()
}

/// Lowercase an entire string respecting locale (like bash `${var,,}`).
pub fn to_lowercase(s: &str, locale: &Locale) -> String {
    let cm = CaseMapper::new();
    cm.lowercase_to_string(s, &locale.id).into_owned()
}

/// Uppercase first character only (like bash `${var^}`).
pub fn capitalize(s: &str, locale: &Locale) -> String {
    if s.is_empty() {
        return String::new();
    }
    let cm = CaseMapper::new();
    // Split at first char boundary
    let first_char_len = s.chars().next().unwrap().len_utf8();
    let (first, rest) = s.split_at(first_char_len);
    let upper_first = cm.uppercase_to_string(first, &locale.id);
    let mut result = String::with_capacity(upper_first.len() + rest.len());
    result.push_str(&upper_first);
    result.push_str(rest);
    result
}

/// Lowercase first character only (like bash `${var,}`).
pub fn uncapitalize(s: &str, locale: &Locale) -> String {
    if s.is_empty() {
        return String::new();
    }
    let cm = CaseMapper::new();
    let first_char_len = s.chars().next().unwrap().len_utf8();
    let (first, rest) = s.split_at(first_char_len);
    let lower_first = cm.lowercase_to_string(first, &locale.id);
    let mut result = String::with_capacity(lower_first.len() + rest.len());
    result.push_str(&lower_first);
    result.push_str(rest);
    result
}

/// Resolve LC_NUMERIC (decimal separator, grouping).
pub fn numeric_locale(env: &Environment) -> Locale {
    resolve_locale(env, "LC_NUMERIC")
}

/// Return the decimal separator character for the given locale.
///
/// Formats `1.5` with ICU4X and extracts the non-digit character
/// between `1` and `5`. Falls back to `'.'`.
pub fn decimal_separator(locale: &Locale) -> char {
    use icu::decimal::input::Decimal;
    use icu::decimal::DecimalFormatter;

    let prefs = icu::decimal::DecimalFormatterPreferences::from(&locale.id);
    if let Ok(fmt) = DecimalFormatter::try_new(prefs, Default::default()) {
        // Build 1.5 as a Decimal
        let dec = "1.5".parse::<Decimal>().unwrap();
        let s = fmt.format_to_string(&dec);
        // Extract the separator (the non-digit between '1' and '5')
        for ch in s.chars() {
            if !ch.is_ascii_digit() {
                return ch;
            }
        }
    }
    '.'
}

/// Parse a POSIX locale string (e.g., "en_US.UTF-8", "tr_TR", "C") into an ICU Locale.
///
/// Handles common formats:
/// - "C" or "POSIX" -> root locale (und)
/// - "en_US.UTF-8" -> language "en", region "US" (charset ignored)
/// - "tr_TR" -> language "tr", region "TR"
/// - "de" -> language "de"
pub(crate) fn parse_posix_locale(s: &str) -> Locale {
    let s = s.trim();
    if s.is_empty() || s == "C" || s == "POSIX" {
        return Locale::UNKNOWN; // root/undefined locale
    }
    // Strip charset suffix (e.g., ".UTF-8")
    let without_charset = s.split('.').next().unwrap_or(s);
    // Try to parse as ICU locale (POSIX uses '_', ICU/BCP47 uses '-')
    let normalized = without_charset.replace('_', "-");
    normalized.parse::<Locale>().unwrap_or(Locale::UNKNOWN)
}

#[cfg(test)]
#[path = "locale_tests.rs"]
mod tests;
