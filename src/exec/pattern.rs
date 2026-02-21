/// Simple shell pattern matching for `case` arms.
///
/// Supports `*`, `?`, and character classes `[...]`.
/// Does not support extended globs.
pub(super) fn shell_pattern_match(text: &str, pattern: &str) -> bool {
    let text = text.as_bytes();
    let pattern = pattern.as_bytes();
    match_pattern(text, pattern, 0, 0)
}

fn match_pattern(text: &[u8], pattern: &[u8], mut ti: usize, mut pi: usize) -> bool {
    while pi < pattern.len() {
        if ti < text.len() && pattern[pi] == b'?' {
            ti += 1;
            pi += 1;
        } else if pattern[pi] == b'*' {
            // Skip consecutive stars
            while pi < pattern.len() && pattern[pi] == b'*' {
                pi += 1;
            }
            if pi == pattern.len() {
                return true;
            }
            // Try matching the rest at each position
            for i in ti..=text.len() {
                if match_pattern(text, pattern, i, pi) {
                    return true;
                }
            }
            return false;
        } else if pattern[pi] == b'[' {
            // Character class
            pi += 1;
            let negate = pi < pattern.len() && (pattern[pi] == b'!' || pattern[pi] == b'^');
            if negate {
                pi += 1;
            }

            let mut matched = false;
            let mut first = true;
            while pi < pattern.len() && (first || pattern[pi] != b']') {
                first = false;
                if pi + 2 < pattern.len() && pattern[pi + 1] == b'-' {
                    // Range: [a-z]
                    if ti < text.len() && text[ti] >= pattern[pi] && text[ti] <= pattern[pi + 2] {
                        matched = true;
                    }
                    pi += 3;
                } else {
                    if ti < text.len() && text[ti] == pattern[pi] {
                        matched = true;
                    }
                    pi += 1;
                }
            }
            if pi < pattern.len() && pattern[pi] == b']' {
                pi += 1;
            }
            if negate {
                matched = !matched;
            }
            if !matched || ti >= text.len() {
                return false;
            }
            ti += 1;
        } else if ti < text.len() && pattern[pi] == text[ti] {
            ti += 1;
            pi += 1;
        } else {
            return false;
        }
    }
    ti == text.len()
}

/// Remove the shortest matching prefix from `text`.
///
/// Used by `${var#pattern}`. Tries `text[..i]` for increasing `i` and returns
/// `text[i..]` on the first match. Returns the original text if no prefix matches.
pub(super) fn trim_smallest_prefix<'a>(text: &'a str, pattern: &str) -> &'a str {
    for i in 0..=text.len() {
        if text.is_char_boundary(i) && shell_pattern_match(&text[..i], pattern) {
            return &text[i..];
        }
    }
    text
}

/// Remove the longest matching prefix from `text`.
///
/// Used by `${var##pattern}`. Tries `text[..i]` for decreasing `i` and returns
/// `text[i..]` on the first match.
pub(super) fn trim_largest_prefix<'a>(text: &'a str, pattern: &str) -> &'a str {
    for i in (0..=text.len()).rev() {
        if text.is_char_boundary(i) && shell_pattern_match(&text[..i], pattern) {
            return &text[i..];
        }
    }
    text
}

/// Remove the shortest matching suffix from `text`.
///
/// Used by `${var%pattern}`. Tries `text[i..]` for decreasing `i` (starting
/// from the end) and returns `text[..i]` on the first match.
pub(super) fn trim_smallest_suffix<'a>(text: &'a str, pattern: &str) -> &'a str {
    for i in (0..=text.len()).rev() {
        if text.is_char_boundary(i) && shell_pattern_match(&text[i..], pattern) {
            return &text[..i];
        }
    }
    text
}

/// Remove the longest matching suffix from `text`.
///
/// Used by `${var%%pattern}`. Tries `text[i..]` for increasing `i` and returns
/// `text[..i]` on the first match.
pub(super) fn trim_largest_suffix<'a>(text: &'a str, pattern: &str) -> &'a str {
    for i in 0..=text.len() {
        if text.is_char_boundary(i) && shell_pattern_match(&text[i..], pattern) {
            return &text[..i];
        }
    }
    text
}
