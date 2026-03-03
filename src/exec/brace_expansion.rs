//! Brace expansion: `{a,b,c}` comma lists and `{1..5}` sequences.
//!
//! Brace expansion turns one word into multiple words and must happen before
//! all other expansions (parameter, tilde, command substitution). The main
//! entry point is [`expand_braces`], which takes a word's fragment list and
//! returns one or more fragment lists via cartesian product.

use crate::ast::{BraceExpansionKind, Fragment};

/// Expand all `BashBraceExpansion` fragments in a word, returning one or more
/// fragment lists. Each returned list becomes a separate word that undergoes
/// normal expansion afterwards.
///
/// Non-brace fragments pass through unchanged. Multiple brace fragments in
/// the same word produce a cartesian product. Invalid sequences (unparseable
/// start/end, step=0, wrong-sign step) fall back to the literal brace text.
pub fn expand_braces(fragments: &[Fragment]) -> Vec<Vec<Fragment>> {
    let mut alternatives: Vec<Vec<Fragment>> = vec![vec![]];

    for fragment in fragments {
        match fragment {
            Fragment::BashBraceExpansion(BraceExpansionKind::List(items)) => {
                let mut new_alternatives = Vec::new();
                for prefix in &alternatives {
                    for item_fragments in items {
                        // Recursively expand nested braces in each list item.
                        let expanded_items = expand_braces(item_fragments);
                        for expanded in expanded_items {
                            let mut combined = prefix.clone();
                            combined.extend(expanded);
                            new_alternatives.push(combined);
                        }
                    }
                }
                alternatives = new_alternatives;
            }
            Fragment::BashBraceExpansion(BraceExpansionKind::Sequence { start, end, step }) => {
                let items = generate_sequence(start, end, step.as_deref());
                if items.is_empty() {
                    // Invalid sequence — fall back to literal text.
                    let literal = sequence_to_literal(start, end, step.as_deref());
                    for alt in &mut alternatives {
                        alt.push(Fragment::Literal(literal.clone()));
                    }
                } else {
                    let mut new_alternatives = Vec::new();
                    for prefix in &alternatives {
                        for item in &items {
                            let mut combined = prefix.clone();
                            combined.push(Fragment::Literal(item.clone()));
                            new_alternatives.push(combined);
                        }
                    }
                    alternatives = new_alternatives;
                }
            }
            other => {
                for alt in &mut alternatives {
                    alt.push(other.clone());
                }
            }
        }
    }

    alternatives
}

/// Reconstruct the literal text of a `BraceExpansionKind` for contexts where
/// brace expansion does not apply (assignments, redirects, double quotes).
///
/// Inner fragments (e.g. parameters inside `{$a,b}`) are expanded by the
/// caller's `expand_fragment` — this function only reconstructs the brace
/// structure around them.
pub fn kind_to_literal(kind: &BraceExpansionKind) -> String {
    match kind {
        BraceExpansionKind::List(items) => {
            let mut s = String::from("{");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                for frag in item {
                    fragment_to_literal(frag, &mut s);
                }
            }
            s.push('}');
            s
        }
        BraceExpansionKind::Sequence { start, end, step } => sequence_to_literal(start, end, step.as_deref()),
    }
}

// Sequence generation =================================================================================================

/// Generate a brace sequence expansion. Returns an empty vec for invalid inputs.
fn generate_sequence(start: &str, end: &str, step: Option<&str>) -> Vec<String> {
    // Try numeric first.
    if let Some(result) = try_numeric_sequence(start, end, step) {
        return result;
    }
    // Try character range.
    if let Some(result) = try_char_sequence(start, end, step) {
        return result;
    }
    // Invalid — caller will use literal fallback.
    Vec::new()
}

fn try_numeric_sequence(start: &str, end: &str, step: Option<&str>) -> Option<Vec<String>> {
    let s: i64 = start.parse().ok()?;
    let e: i64 = end.parse().ok()?;

    let step_val: i64 = match step {
        Some(st) => st.parse().ok()?,
        None => {
            if s <= e {
                1
            } else {
                -1
            }
        }
    };

    // Step of 0 is invalid.
    if step_val == 0 {
        return Some(Vec::new());
    }

    // Step sign must match direction (unless singleton).
    if s != e {
        let ascending = s < e;
        if ascending && step_val < 0 {
            return Some(Vec::new());
        }
        if !ascending && step_val > 0 {
            return Some(Vec::new());
        }
    }

    // Determine zero-padding width.
    let pad_width = std::cmp::max(displayed_width(start, s), displayed_width(end, e));

    let mut result = Vec::new();
    let mut i = s;
    if step_val > 0 {
        while i <= e {
            result.push(format_padded(i, pad_width));
            i += step_val;
        }
    } else {
        while i >= e {
            result.push(format_padded(i, pad_width));
            i += step_val;
        }
    }

    Some(result)
}

/// Compute the display width for zero-padding. If the string has a leading zero
/// (and is more than one character, excluding the sign), return its length.
fn displayed_width(text: &str, _value: i64) -> usize {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.len() > 1 && digits.starts_with('0') {
        text.len()
    } else {
        0
    }
}

/// Format a number with optional zero-padding.
fn format_padded(value: i64, width: usize) -> String {
    if width == 0 {
        return value.to_string();
    }
    if value < 0 {
        // Negative: pad after the sign. E.g., width=4 → "-007"
        format!("-{:0>width$}", -value, width = width - 1)
    } else {
        format!("{:0>width$}", value, width = width)
    }
}

fn try_char_sequence(start: &str, end: &str, step: Option<&str>) -> Option<Vec<String>> {
    if start.len() != 1 || end.len() != 1 {
        return None;
    }

    let s = start.chars().next()?;
    let e = end.chars().next()?;

    // Both must be ASCII letters of the same case, or both ASCII digits.
    let valid =
        (s.is_ascii_lowercase() && e.is_ascii_lowercase()) || (s.is_ascii_uppercase() && e.is_ascii_uppercase());
    if !valid {
        return None;
    }

    let s_val = s as i64;
    let e_val = e as i64;

    let step_val: i64 = match step {
        Some(st) => st.parse().ok()?,
        None => {
            if s_val <= e_val {
                1
            } else {
                -1
            }
        }
    };

    if step_val == 0 {
        return Some(Vec::new());
    }

    // Step sign must match direction (unless singleton).
    if s_val != e_val {
        let ascending = s_val < e_val;
        if ascending && step_val < 0 {
            return Some(Vec::new());
        }
        if !ascending && step_val > 0 {
            return Some(Vec::new());
        }
    }

    let mut result = Vec::new();
    let mut i = s_val;
    if step_val > 0 {
        while i <= e_val {
            if let Some(ch) = char::from_u32(i as u32) {
                result.push(ch.to_string());
            }
            i += step_val;
        }
    } else {
        while i >= e_val {
            if let Some(ch) = char::from_u32(i as u32) {
                result.push(ch.to_string());
            }
            i += step_val;
        }
    }

    Some(result)
}

// Literal reconstruction ==============================================================================================

fn sequence_to_literal(start: &str, end: &str, step: Option<&str>) -> String {
    let mut s = format!("{{{start}..{end}");
    if let Some(st) = step {
        s.push_str("..");
        s.push_str(st);
    }
    s.push('}');
    s
}

/// Best-effort literal representation of a fragment (for fallback paths).
fn fragment_to_literal(frag: &Fragment, out: &mut String) {
    match frag {
        Fragment::Literal(s) => out.push_str(s),
        Fragment::SingleQuoted(s) => {
            out.push('\'');
            out.push_str(s);
            out.push('\'');
        }
        Fragment::BashBraceExpansion(kind) => out.push_str(&kind_to_literal(kind)),
        _ => {} // Other fragments handled by the caller's expand_fragment
    }
}
