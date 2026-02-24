//! Shell glob-pattern matching for `case` arms and `${var%pattern}` trimming.
//! Supports `*`, `?`, character classes `[...]` with POSIX `[:name:]` classes,
//! and backslash escapes; no extglobs.
//!
//! Operates on `char` boundaries for Unicode correctness. Character class
//! membership is locale-sensitive via [`super::locale::is_char_class`].

use icu::locale::Locale;

/// Shell pattern matching for `case` arms, `[[ == ]]`, and parameter trimming.
///
/// Supports `*`, `?`, `[...]` bracket expressions (with ranges, negation,
/// and POSIX `[:name:]` character classes), and `\` escapes.
pub(super) fn shell_pattern_match(text: &str, pattern: &str, locale: &Locale) -> bool {
    match_inner(pattern, text, locale)
}

/// Recursive character-by-character matcher.
///
/// Walks `pattern` and `text` as byte-indexed slices, extracting chars at
/// each position for Unicode-safe comparison.
fn match_inner(pattern: &str, text: &str, locale: &Locale) -> bool {
    let mut pi = 0; // pattern byte index
    let mut ti = 0; // text byte index

    while pi < pattern.len() {
        let pc = pattern[pi..].chars().next().unwrap();
        match pc {
            '*' => {
                pi += 1;
                // Skip consecutive stars
                while pi < pattern.len() && pattern.as_bytes()[pi] == b'*' {
                    pi += 1;
                }
                if pi >= pattern.len() {
                    return true; // trailing * matches everything
                }
                // Try matching rest of pattern at every char position in remaining text
                let mut try_ti = ti;
                while try_ti <= text.len() {
                    if match_inner(&pattern[pi..], &text[try_ti..], locale) {
                        return true;
                    }
                    if try_ti == text.len() {
                        break;
                    }
                    try_ti += text[try_ti..].chars().next().unwrap().len_utf8();
                }
                return false;
            }
            '?' => {
                if ti >= text.len() {
                    return false;
                }
                let tc = text[ti..].chars().next().unwrap();
                pi += pc.len_utf8();
                ti += tc.len_utf8();
            }
            '[' => {
                if ti >= text.len() {
                    return false;
                }
                let tc = text[ti..].chars().next().unwrap();
                match match_bracket(&pattern[pi..], tc, locale) {
                    Some(bracket_len) => {
                        pi += bracket_len;
                        ti += tc.len_utf8();
                    }
                    None => {
                        // No closing ] found or didn't match — treat `[` as literal
                        return false;
                    }
                }
            }
            '\\' => {
                // Escaped character — match next char literally
                pi += 1;
                if pi >= pattern.len() {
                    return false;
                }
                let escaped = pattern[pi..].chars().next().unwrap();
                if ti >= text.len() {
                    return false;
                }
                let tc = text[ti..].chars().next().unwrap();
                if escaped != tc {
                    return false;
                }
                pi += escaped.len_utf8();
                ti += tc.len_utf8();
            }
            _ => {
                // Literal character match
                if ti >= text.len() {
                    return false;
                }
                let tc = text[ti..].chars().next().unwrap();
                if pc != tc {
                    return false;
                }
                pi += pc.len_utf8();
                ti += tc.len_utf8();
            }
        }
    }
    ti >= text.len() // pattern consumed — text must also be fully consumed
}

/// Match a character against a bracket expression `[...]`.
///
/// `pattern` starts at the opening `[`. Returns `Some(bytes_consumed)` if the
/// bracket expression is well-formed (has a closing `]`) and the character
/// matches, or `None` if the bracket is malformed (no closing `]`) or the
/// character does not match.
fn match_bracket(pattern: &str, ch: char, locale: &Locale) -> Option<usize> {
    debug_assert!(pattern.starts_with('['));
    let bytes = pattern.as_bytes();
    let mut i = 1; // skip opening [

    // Check for negation
    let negated = i < bytes.len() && (bytes[i] == b'!' || bytes[i] == b'^');
    if negated {
        i += 1;
    }

    // POSIX quirk: `]` immediately after `[` or `[!`/`[^` is a literal character,
    // not the end of the bracket expression.
    let bracket_start = i;

    let mut matched = false;

    while i < bytes.len() {
        let c = pattern[i..].chars().next().unwrap();
        let c_len = c.len_utf8();

        // `]` closes the bracket unless it's the very first content character
        if c == ']' && i > bracket_start {
            return if negated { !matched } else { matched }.then_some(i + 1);
        }

        // POSIX character class: [:name:]
        if c == '[' && i + 1 < bytes.len() && bytes[i + 1] == b':' {
            let class_start = i + 2;
            if let Some(rel_end) = pattern[class_start..].find(":]") {
                let class_name = &pattern[class_start..class_start + rel_end];
                if super::locale::is_char_class(ch, class_name, locale) {
                    matched = true;
                }
                i = class_start + rel_end + 2; // skip past `:]`
                continue;
            }
        }

        // Range: `a-z` (but only if `-` is followed by a non-`]` character)
        if i + c_len < bytes.len() && bytes[i + c_len] == b'-' && i + c_len + 1 < bytes.len() {
            let end_char = pattern[i + c_len + 1..].chars().next().unwrap();
            if end_char != ']' {
                if ch >= c && ch <= end_char {
                    matched = true;
                }
                i += c_len + 1 + end_char.len_utf8();
                continue;
            }
        }

        // Literal character in bracket
        if ch == c {
            matched = true;
        }
        i += c_len;
    }

    // No closing `]` found — malformed bracket expression
    None
}

/// Remove the shortest matching prefix from `text`.
///
/// Used by `${var#pattern}`. Tries `text[..i]` for increasing `i` and returns
/// `text[i..]` on the first match. Returns the original text if no prefix matches.
pub(super) fn trim_smallest_prefix<'a>(text: &'a str, pattern: &str, locale: &Locale) -> &'a str {
    for i in 0..=text.len() {
        if text.is_char_boundary(i) && shell_pattern_match(&text[..i], pattern, locale) {
            return &text[i..];
        }
    }
    text
}

/// Remove the longest matching prefix from `text`.
///
/// Used by `${var##pattern}`. Tries `text[..i]` for decreasing `i` and returns
/// `text[i..]` on the first match.
pub(super) fn trim_largest_prefix<'a>(text: &'a str, pattern: &str, locale: &Locale) -> &'a str {
    for i in (0..=text.len()).rev() {
        if text.is_char_boundary(i) && shell_pattern_match(&text[..i], pattern, locale) {
            return &text[i..];
        }
    }
    text
}

/// Remove the shortest matching suffix from `text`.
///
/// Used by `${var%pattern}`. Tries `text[i..]` for decreasing `i` (starting
/// from the end) and returns `text[..i]` on the first match.
pub(super) fn trim_smallest_suffix<'a>(text: &'a str, pattern: &str, locale: &Locale) -> &'a str {
    for i in (0..=text.len()).rev() {
        if text.is_char_boundary(i) && shell_pattern_match(&text[i..], pattern, locale) {
            return &text[..i];
        }
    }
    text
}

/// Remove the longest matching suffix from `text`.
///
/// Used by `${var%%pattern}`. Tries `text[i..]` for increasing `i` and returns
/// `text[..i]` on the first match.
pub(super) fn trim_largest_suffix<'a>(text: &'a str, pattern: &str, locale: &Locale) -> &'a str {
    for i in 0..=text.len() {
        if text.is_char_boundary(i) && shell_pattern_match(&text[i..], pattern, locale) {
            return &text[..i];
        }
    }
    text
}
