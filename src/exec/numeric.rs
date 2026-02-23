//! Shell-style integer parsing with hex, octal, and character-code support.
//!
//! Bash accepts `0xff`, `077`, and `'A` as integer arguments. This module
//! provides the canonical `parse_shell_int` used by both arithmetic evaluation
//! and printf.

/// Shell-style integer parsing: decimal, hex (0x/0X), octal (leading 0),
/// character code ('A / "A), optional sign, leading whitespace.
///
/// Returns `Ok(value)` or `Err(())` on parse failure.
pub(crate) fn parse_shell_int(s: &str) -> Result<i64, ()> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }

    // Character code: 'X or "X
    if (s.starts_with('\'') || s.starts_with('"')) && s.len() >= 2 {
        return Ok(s.as_bytes()[1] as i64);
    }

    // Sign
    let (negative, digits) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest.trim_start())
    } else if let Some(rest) = s.strip_prefix('+') {
        (false, rest.trim_start())
    } else {
        (false, s)
    };

    // Radix detection
    let abs = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16)
    } else if digits.starts_with('0')
        && digits.len() > 1
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        i64::from_str_radix(digits, 8)
    } else {
        digits.parse::<i64>()
    };

    abs.map(|v| if negative { v.wrapping_neg() } else { v })
        .map_err(|_| ())
}

#[cfg(test)]
#[path = "numeric_tests.rs"]
mod tests;
