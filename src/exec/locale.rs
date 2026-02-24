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

/// Check if a character belongs to a POSIX character class, respecting locale.
///
/// In C/POSIX locale, only ASCII characters match locale-sensitive classes
/// (`upper`, `lower`, `alpha`, `alnum`, `punct`, `graph`, `print`).
/// In UTF-8 locales, full Unicode classification applies via ICU4X.
/// Locale-invariant classes (`digit`, `xdigit`, `space`, `blank`, `cntrl`)
/// always use the same rules regardless of locale.
#[allow(dead_code)] // Called by pattern.rs in a follow-up commit
pub fn is_char_class(ch: char, class: &str, locale: &Locale) -> bool {
    match class {
        "upper" => is_upper(ch, locale),
        "lower" => is_lower(ch, locale),
        "alpha" => is_upper(ch, locale) || is_lower(ch, locale) || is_title_or_other_letter(ch, locale),
        "digit" => ch.is_ascii_digit(),
        "alnum" => is_char_class(ch, "alpha", locale) || ch.is_ascii_digit(),
        "space" => ch.is_ascii_whitespace() || (!is_c_locale(locale) && ch.is_whitespace()),
        "blank" => ch == ' ' || ch == '\t',
        "punct" => is_punct(ch, locale),
        "graph" => is_char_class(ch, "alnum", locale) || is_char_class(ch, "punct", locale),
        "print" => is_char_class(ch, "graph", locale) || ch == ' ',
        "cntrl" => ch.is_ascii_control(),
        "xdigit" => ch.is_ascii_hexdigit(),
        _ => false,
    }
}

fn is_c_locale(locale: &Locale) -> bool {
    locale == &Locale::UNKNOWN || locale.id.language.is_unknown()
}

fn is_upper(ch: char, locale: &Locale) -> bool {
    if is_c_locale(locale) {
        ch.is_ascii_uppercase()
    } else {
        use icu::properties::props::GeneralCategory;
        use icu::properties::CodePointMapData;
        let gc = CodePointMapData::<GeneralCategory>::new();
        gc.get(ch) == GeneralCategory::UppercaseLetter
    }
}

fn is_lower(ch: char, locale: &Locale) -> bool {
    if is_c_locale(locale) {
        ch.is_ascii_lowercase()
    } else {
        use icu::properties::props::GeneralCategory;
        use icu::properties::CodePointMapData;
        let gc = CodePointMapData::<GeneralCategory>::new();
        gc.get(ch) == GeneralCategory::LowercaseLetter
    }
}

fn is_title_or_other_letter(ch: char, locale: &Locale) -> bool {
    if is_c_locale(locale) {
        false // C locale: only ASCII letters count
    } else {
        use icu::properties::props::GeneralCategory;
        use icu::properties::CodePointMapData;
        let gc = CodePointMapData::<GeneralCategory>::new();
        matches!(
            gc.get(ch),
            GeneralCategory::TitlecaseLetter | GeneralCategory::ModifierLetter | GeneralCategory::OtherLetter
        )
    }
}

fn is_punct(ch: char, locale: &Locale) -> bool {
    if is_c_locale(locale) {
        ch.is_ascii_punctuation()
    } else {
        use icu::properties::props::GeneralCategory;
        use icu::properties::CodePointMapData;
        let gc = CodePointMapData::<GeneralCategory>::new();
        matches!(
            gc.get(ch),
            GeneralCategory::DashPunctuation
                | GeneralCategory::OpenPunctuation
                | GeneralCategory::ClosePunctuation
                | GeneralCategory::ConnectorPunctuation
                | GeneralCategory::OtherPunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
                | GeneralCategory::MathSymbol
                | GeneralCategory::CurrencySymbol
                | GeneralCategory::ModifierSymbol
                | GeneralCategory::OtherSymbol
        )
    }
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
