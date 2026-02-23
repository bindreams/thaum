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
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        assert_eq!(parse_shell_int(""), Ok(0));
    }

    #[test]
    fn test_whitespace() {
        assert_eq!(parse_shell_int("  "), Ok(0));
    }

    #[test]
    fn test_decimal() {
        assert_eq!(parse_shell_int("42"), Ok(42));
    }

    #[test]
    fn test_negative() {
        assert_eq!(parse_shell_int("-7"), Ok(-7));
    }

    #[test]
    fn test_hex() {
        assert_eq!(parse_shell_int("0xff"), Ok(255));
        assert_eq!(parse_shell_int("0XFF"), Ok(255));
    }

    #[test]
    fn test_octal() {
        assert_eq!(parse_shell_int("077"), Ok(63));
    }

    #[test]
    fn test_char_code() {
        assert_eq!(parse_shell_int("'A"), Ok(65));
        assert_eq!(parse_shell_int("\"A"), Ok(65));
    }

    #[test]
    fn test_invalid() {
        assert_eq!(parse_shell_int("abc"), Err(()));
    }
}
