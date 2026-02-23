//! Printf builtin formatter.
//!
//! Implements the full `printf FORMAT [ARGUMENTS...]` semantics matching
//! bash behaviour: custom formatting (not delegating to Rust's `format!`),
//! cyclic argument reuse, backslash escapes, `%b`, `%q`, `%(...)T`, numeric
//! argument parsing (hex, octal, char-code), and the `-v VAR` option.

use std::io::Write;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Format `fmt` with `args` into `out`, cycling through the format string
/// until all arguments are consumed. Returns 0 on success, 1 on any error.
pub fn printf_format(fmt: &str, args: &[String], out: &mut dyn Write) -> i32 {
    let mut status = 0;
    let mut arg_idx: usize = 0;
    let args_len = args.len();

    loop {
        let start_idx = arg_idx;
        let (st, stop) = format_once(fmt, args, &mut arg_idx, out);
        if st != 0 {
            status = st;
        }
        if stop {
            break;
        }
        // If no arguments remain unconsumed, or no specifiers consumed any
        // argument during this pass, stop to avoid infinite loop.
        if arg_idx >= args_len || arg_idx == start_idx {
            break;
        }
    }

    status
}

/// Run through the format string once.  Returns (status, stop_early).
/// `stop_early` is true if `\c` was encountered.
fn format_once(
    fmt: &str,
    args: &[String],
    arg_idx: &mut usize,
    out: &mut dyn Write,
) -> (i32, bool) {
    let mut status = 0;
    let chars: Vec<char> = fmt.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        match chars[i] {
            '\\' => {
                i += 1;
                if i >= len {
                    let _ = out.write_all(b"\\");
                    break;
                }
                let (bytes, advance, stop) = interpret_one_escape(&chars[i..]);
                let _ = out.write_all(&bytes);
                i += advance;
                if stop {
                    return (status, true);
                }
            }
            '%' => {
                i += 1;
                if i >= len {
                    let _ = out.write_all(b"%");
                    break;
                }
                if chars[i] == '%' {
                    let _ = out.write_all(b"%");
                    i += 1;
                    continue;
                }
                // Check for %(...)T strftime
                if chars[i] == '(' {
                    let (st, advance) = handle_strftime(&chars[i..], args, arg_idx, out);
                    if st != 0 {
                        status = st;
                    }
                    i += advance;
                    continue;
                }
                let (spec, advance) = parse_format_spec(&chars[i..], args, arg_idx);
                i += advance;
                let st = apply_format_spec(&spec, args, arg_idx, out);
                if st != 0 {
                    status = st;
                }
            }
            ch => {
                let mut buf = [0u8; 4];
                let _ = out.write_all(ch.encode_utf8(&mut buf).as_bytes());
                i += 1;
            }
        }
    }

    (status, false)
}

// ---------------------------------------------------------------------------
// Format spec
// ---------------------------------------------------------------------------

struct FormatSpec {
    left_align: bool,
    zero_pad: bool,
    plus_sign: bool,
    space_sign: bool,
    hash: bool,
    width: Option<usize>,
    precision: Option<usize>,
    conversion: char,
}

/// Parse a format specifier starting *after* the '%' character.
/// Returns the spec and the number of chars consumed.
fn parse_format_spec(chars: &[char], args: &[String], arg_idx: &mut usize) -> (FormatSpec, usize) {
    let mut spec = FormatSpec {
        left_align: false,
        zero_pad: false,
        plus_sign: false,
        space_sign: false,
        hash: false,
        width: None,
        precision: None,
        conversion: 's',
    };
    let len = chars.len();
    let mut i = 0;

    // Flags
    while i < len {
        match chars[i] {
            '-' => spec.left_align = true,
            '0' => spec.zero_pad = true,
            '+' => spec.plus_sign = true,
            ' ' => spec.space_sign = true,
            '#' => spec.hash = true,
            _ => break,
        }
        i += 1;
    }

    // Width
    if i < len && chars[i] == '*' {
        let w = get_arg_str(args, arg_idx);
        spec.width = Some(parse_int_arg(&w).0.unsigned_abs() as usize);
        i += 1;
    } else {
        let (val, adv) = parse_decimal_digits(&chars[i..]);
        if adv > 0 {
            spec.width = Some(val);
            i += adv;
        }
    }

    // Precision
    if i < len && chars[i] == '.' {
        i += 1;
        if i < len && chars[i] == '*' {
            let p = get_arg_str(args, arg_idx);
            spec.precision = Some(parse_int_arg(&p).0.unsigned_abs() as usize);
            i += 1;
        } else {
            let (val, adv) = parse_decimal_digits(&chars[i..]);
            spec.precision = Some(val);
            i += adv;
        }
    }

    // Conversion
    if i < len {
        spec.conversion = chars[i];
        i += 1;
    }

    (spec, i)
}

/// Parse consecutive decimal digits. Returns (value, chars_consumed).
fn parse_decimal_digits(chars: &[char]) -> (usize, usize) {
    let mut val: usize = 0;
    let mut count = 0;
    for &ch in chars {
        if ch.is_ascii_digit() {
            val = val * 10 + (ch as usize - '0' as usize);
            count += 1;
        } else {
            break;
        }
    }
    (val, count)
}

// ---------------------------------------------------------------------------
// Apply a format spec
// ---------------------------------------------------------------------------

fn apply_format_spec(
    spec: &FormatSpec,
    args: &[String],
    arg_idx: &mut usize,
    out: &mut dyn Write,
) -> i32 {
    match spec.conversion {
        's' => {
            let arg = get_arg_str(args, arg_idx);
            format_string(spec, &arg, out);
            0
        }
        'd' | 'i' => {
            let arg = get_arg_str(args, arg_idx);
            format_signed_int(spec, &arg, out)
        }
        'u' => {
            let arg = get_arg_str(args, arg_idx);
            format_unsigned_int(spec, &arg, out)
        }
        'x' => {
            let arg = get_arg_str(args, arg_idx);
            format_hex(spec, &arg, false, out)
        }
        'X' => {
            let arg = get_arg_str(args, arg_idx);
            format_hex(spec, &arg, true, out)
        }
        'o' => {
            let arg = get_arg_str(args, arg_idx);
            format_octal(spec, &arg, out)
        }
        'f' | 'e' | 'E' | 'g' | 'G' => {
            let arg = get_arg_str(args, arg_idx);
            format_float(spec, &arg, spec.conversion, out)
        }
        'c' => {
            let arg = get_arg_str(args, arg_idx);
            format_char(spec, &arg, out);
            0
        }
        'q' => {
            let arg = get_arg_str(args, arg_idx);
            format_shell_quote(&arg, out);
            0
        }
        'b' => {
            let arg = get_arg_str(args, arg_idx);
            let stop = format_backslash_b(&arg, out);
            if stop {
                1
            } else {
                0
            }
        }
        _ => {
            // Unknown conversion — output literally
            let _ = write!(out, "%{}", spec.conversion);
            0
        }
    }
}

// ---------------------------------------------------------------------------
// Argument access
// ---------------------------------------------------------------------------

/// Get the next argument as a string, advancing the index.
/// Returns "" if arguments are exhausted (bash default for missing args).
fn get_arg_str(args: &[String], idx: &mut usize) -> String {
    if *idx < args.len() {
        let s = args[*idx].clone();
        *idx += 1;
        s
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// Numeric argument parsing
// ---------------------------------------------------------------------------

/// Parse an argument as an integer, handling hex (0x), octal (leading 0),
/// character code ('A), signs, and leading whitespace.
/// Returns (value, had_error).
fn parse_int_arg(s: &str) -> (i64, bool) {
    let s = s.trim();
    if s.is_empty() {
        return (0, false);
    }

    // Character code: 'X or "X
    if (s.starts_with('\'') || s.starts_with('"')) && s.len() >= 2 {
        let ch = s.as_bytes()[1];
        return (ch as i64, false);
    }

    // Determine sign
    let (negative, digits) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = s.strip_prefix('+') {
        (false, rest)
    } else {
        (false, s)
    };

    let result = if let Some(hex) = digits
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

    match result {
        Ok(v) => {
            let v = if negative { -v } else { v };
            (v, false)
        }
        Err(_) => (0, true),
    }
}

/// Parse an argument as a float. Same prefix handling as int but falls
/// through to f64 parsing.
fn parse_float_arg(s: &str) -> (f64, bool) {
    let s = s.trim();
    if s.is_empty() {
        return (0.0, false);
    }

    // Character code
    if (s.starts_with('\'') || s.starts_with('"')) && s.len() >= 2 {
        let ch = s.as_bytes()[1];
        return (ch as f64, false);
    }

    // Hex/octal integers should parse as float
    if s.starts_with("0x") || s.starts_with("0X") || s.starts_with("-0x") || s.starts_with("+0x") {
        let (v, err) = parse_int_arg(s);
        return (v as f64, err);
    }

    match s.parse::<f64>() {
        Ok(v) => (v, false),
        Err(_) => (0.0, true),
    }
}

// ---------------------------------------------------------------------------
// Escape interpretation (shared by format string and %b)
// ---------------------------------------------------------------------------

/// Interpret a single escape sequence starting at chars[0] (the char after '\').
/// Returns (bytes_to_write, chars_consumed, stop_processing).
fn interpret_one_escape(chars: &[char]) -> (Vec<u8>, usize, bool) {
    if chars.is_empty() {
        return (vec![b'\\'], 0, false);
    }
    match chars[0] {
        'a' => (vec![0x07], 1, false),
        'b' => (vec![0x08], 1, false),
        'f' => (vec![0x0C], 1, false),
        'n' => (vec![b'\n'], 1, false),
        'r' => (vec![b'\r'], 1, false),
        't' => (vec![b'\t'], 1, false),
        'v' => (vec![0x0B], 1, false),
        '\\' => (vec![b'\\'], 1, false),
        '\'' => (vec![b'\''], 1, false),
        '"' => (vec![b'"'], 1, false),
        'c' => (Vec::new(), 1, true), // stop processing
        'x' => {
            // \xHH — one or two hex digits
            let mut val: u8 = 0;
            let mut consumed = 1; // the 'x'
            for &ch in chars[1..].iter().take(2) {
                if let Some(d) = ch.to_digit(16) {
                    val = val * 16 + d as u8;
                    consumed += 1;
                } else {
                    break;
                }
            }
            if consumed == 1 {
                // No hex digits — output literal \x
                (vec![b'\\', b'x'], consumed, false)
            } else {
                (vec![val], consumed, false)
            }
        }
        '0' => {
            // \0NNN — one to three octal digits
            let mut val: u8 = 0;
            let mut consumed = 1; // the '0'
            for &ch in chars[1..].iter().take(3) {
                if ('0'..='7').contains(&ch) {
                    val = val * 8 + (ch as u8 - b'0');
                    consumed += 1;
                } else {
                    break;
                }
            }
            (vec![val], consumed, false)
        }
        _ => {
            // Unknown escape — output literal backslash + char
            let mut buf = vec![b'\\'];
            let mut char_buf = [0u8; 4];
            let encoded = chars[0].encode_utf8(&mut char_buf);
            buf.extend_from_slice(encoded.as_bytes());
            (buf, 1, false)
        }
    }
}

/// Interpret escape sequences in `input`, writing to `out`.
/// Returns `false` if `\c` is encountered (meaning: stop all output).
fn interpret_escapes(input: &str, out: &mut dyn Write) -> bool {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '\\' {
            i += 1;
            if i >= len {
                let _ = out.write_all(b"\\");
                break;
            }
            let (bytes, advance, stop) = interpret_one_escape(&chars[i..]);
            let _ = out.write_all(&bytes);
            i += advance;
            if stop {
                return false; // \c encountered
            }
        } else {
            let mut buf = [0u8; 4];
            let encoded = chars[i].encode_utf8(&mut buf);
            let _ = out.write_all(encoded.as_bytes());
            i += 1;
        }
    }

    true // no \c
}

// ---------------------------------------------------------------------------
// Padding helper
// ---------------------------------------------------------------------------

fn pad_and_write(
    content: &str,
    width: Option<usize>,
    left_align: bool,
    pad_char: char,
    out: &mut dyn Write,
) {
    let w = width.unwrap_or(0);
    let content_len = content.len();
    if content_len >= w {
        let _ = out.write_all(content.as_bytes());
        return;
    }
    let padding = w - content_len;
    let pad_str: String = std::iter::repeat_n(pad_char, padding).collect();
    if left_align {
        let _ = out.write_all(content.as_bytes());
        let _ = out.write_all(pad_str.as_bytes());
    } else {
        let _ = out.write_all(pad_str.as_bytes());
        let _ = out.write_all(content.as_bytes());
    }
}

// ---------------------------------------------------------------------------
// String formatting (%s)
// ---------------------------------------------------------------------------

fn format_string(spec: &FormatSpec, arg: &str, out: &mut dyn Write) {
    let truncated = match spec.precision {
        Some(p) if p < arg.len() => &arg[..p],
        _ => arg,
    };
    pad_and_write(truncated, spec.width, spec.left_align, ' ', out);
}

// ---------------------------------------------------------------------------
// Signed integer formatting (%d, %i)
// ---------------------------------------------------------------------------

fn format_signed_int(spec: &FormatSpec, arg: &str, out: &mut dyn Write) -> i32 {
    let (val, had_error) = parse_int_arg(arg);
    let status = if had_error { 1 } else { 0 };

    let is_negative = val < 0;
    let abs_str = if val == i64::MIN {
        // i64::MIN.abs() overflows; handle specially
        "9223372036854775808".to_string()
    } else {
        val.unsigned_abs().to_string()
    };

    // Apply precision: minimum number of digits
    let digits = match spec.precision {
        Some(p) if p > abs_str.len() => {
            let zeros = "0".repeat(p - abs_str.len());
            format!("{}{}", zeros, abs_str)
        }
        _ => abs_str,
    };

    // Build sign prefix
    let sign = if is_negative {
        "-"
    } else if spec.plus_sign {
        "+"
    } else if spec.space_sign {
        " "
    } else {
        ""
    };

    // Determine effective padding
    let total = sign.len() + digits.len();
    let width = spec.width.unwrap_or(0);

    if spec.zero_pad && !spec.left_align && spec.precision.is_none() && total < width {
        // Zero-pad between sign and digits
        let zeros = "0".repeat(width - total);
        let content = format!("{}{}{}", sign, zeros, digits);
        let _ = out.write_all(content.as_bytes());
    } else {
        let content = format!("{}{}", sign, digits);
        pad_and_write(&content, spec.width, spec.left_align, ' ', out);
    }

    status
}

// ---------------------------------------------------------------------------
// Unsigned integer formatting (%u)
// ---------------------------------------------------------------------------

fn format_unsigned_int(spec: &FormatSpec, arg: &str, out: &mut dyn Write) -> i32 {
    let (val, had_error) = parse_int_arg(arg);
    let status = if had_error { 1 } else { 0 };

    // Cast to u64 — negative wraps around
    let uval = val as u64;
    let abs_str = uval.to_string();

    // Apply precision
    let digits = match spec.precision {
        Some(p) if p > abs_str.len() => {
            let zeros = "0".repeat(p - abs_str.len());
            format!("{}{}", zeros, abs_str)
        }
        _ => abs_str,
    };

    let total = digits.len();
    let width = spec.width.unwrap_or(0);

    if spec.zero_pad && !spec.left_align && spec.precision.is_none() && total < width {
        let zeros = "0".repeat(width - total);
        let content = format!("{}{}", zeros, digits);
        let _ = out.write_all(content.as_bytes());
    } else {
        pad_and_write(&digits, spec.width, spec.left_align, ' ', out);
    }

    status
}

// ---------------------------------------------------------------------------
// Hex formatting (%x, %X)
// ---------------------------------------------------------------------------

fn format_hex(spec: &FormatSpec, arg: &str, uppercase: bool, out: &mut dyn Write) -> i32 {
    let (val, had_error) = parse_int_arg(arg);
    let status = if had_error { 1 } else { 0 };

    let uval = val as u64;
    let hex_str = if uppercase {
        format!("{:X}", uval)
    } else {
        format!("{:x}", uval)
    };

    // Apply precision (minimum digits)
    let digits = match spec.precision {
        Some(p) if p > hex_str.len() => {
            let zeros = "0".repeat(p - hex_str.len());
            format!("{}{}", zeros, hex_str)
        }
        _ => hex_str,
    };

    let prefix = if spec.hash && uval != 0 {
        if uppercase {
            "0X"
        } else {
            "0x"
        }
    } else {
        ""
    };

    let total = prefix.len() + digits.len();
    let width = spec.width.unwrap_or(0);

    if spec.zero_pad && !spec.left_align && spec.precision.is_none() && total < width {
        let zeros = "0".repeat(width - total);
        let content = format!("{}{}{}", prefix, zeros, digits);
        let _ = out.write_all(content.as_bytes());
    } else {
        let content = format!("{}{}", prefix, digits);
        pad_and_write(&content, spec.width, spec.left_align, ' ', out);
    }

    status
}

// ---------------------------------------------------------------------------
// Octal formatting (%o)
// ---------------------------------------------------------------------------

fn format_octal(spec: &FormatSpec, arg: &str, out: &mut dyn Write) -> i32 {
    let (val, had_error) = parse_int_arg(arg);
    let status = if had_error { 1 } else { 0 };

    let uval = val as u64;
    let oct_str = format!("{:o}", uval);

    // Apply precision
    let digits = match spec.precision {
        Some(p) if p > oct_str.len() => {
            let zeros = "0".repeat(p - oct_str.len());
            format!("{}{}", zeros, oct_str)
        }
        _ => oct_str,
    };

    // # prefix for octal is "0" (not "0o" like Rust!)
    let prefix = if spec.hash && !digits.starts_with('0') {
        "0"
    } else {
        ""
    };

    let total = prefix.len() + digits.len();
    let width = spec.width.unwrap_or(0);

    if spec.zero_pad && !spec.left_align && spec.precision.is_none() && total < width {
        let zeros = "0".repeat(width - total);
        let content = format!("{}{}{}", prefix, zeros, digits);
        let _ = out.write_all(content.as_bytes());
    } else {
        let content = format!("{}{}", prefix, digits);
        pad_and_write(&content, spec.width, spec.left_align, ' ', out);
    }

    status
}

// ---------------------------------------------------------------------------
// Float formatting (%f, %e, %E, %g, %G)
// ---------------------------------------------------------------------------

fn format_float(spec: &FormatSpec, arg: &str, conv: char, out: &mut dyn Write) -> i32 {
    let (val, had_error) = parse_float_arg(arg);
    let status = if had_error { 1 } else { 0 };

    let prec = spec.precision.unwrap_or(6);

    let formatted = match conv {
        'f' => format!("{:.prec$}", val, prec = prec),
        'e' => format_scientific(val, prec, false),
        'E' => format_scientific(val, prec, true),
        'g' => format_general(val, prec, false),
        'G' => format_general(val, prec, true),
        _ => unreachable!(),
    };

    // Build sign prefix for positive values
    let sign = if !val.is_sign_negative() && !formatted.starts_with('-') {
        if spec.plus_sign {
            "+"
        } else if spec.space_sign {
            " "
        } else {
            ""
        }
    } else {
        ""
    };

    let content = format!("{}{}", sign, formatted);
    let total = content.len();
    let width = spec.width.unwrap_or(0);

    if spec.zero_pad && !spec.left_align && total < width {
        // Zero-pad between sign and digits
        if content.starts_with('-') || content.starts_with('+') || content.starts_with(' ') {
            let (s, rest) = content.split_at(1);
            let zeros = "0".repeat(width - total);
            let padded = format!("{}{}{}", s, zeros, rest);
            let _ = out.write_all(padded.as_bytes());
        } else {
            let zeros = "0".repeat(width - total);
            let padded = format!("{}{}", zeros, content);
            let _ = out.write_all(padded.as_bytes());
        }
    } else {
        pad_and_write(&content, spec.width, spec.left_align, ' ', out);
    }

    status
}

/// Format in scientific notation: e.g. 1.234000e+02
fn format_scientific(val: f64, prec: usize, upper: bool) -> String {
    if val == 0.0 {
        let zeros = "0".repeat(prec);
        let frac = if prec > 0 {
            format!(".{}", zeros)
        } else {
            String::new()
        };
        let e = if upper { 'E' } else { 'e' };
        return format!("0{}{}+00", frac, e);
    }

    let negative = val.is_sign_negative();
    let abs_val = val.abs();
    let exp = abs_val.log10().floor() as i32;
    let mantissa = abs_val / 10f64.powi(exp);

    // Format mantissa with given precision
    let mantissa_str = format!("{:.prec$}", mantissa, prec = prec);

    let e = if upper { 'E' } else { 'e' };
    let sign_char = if exp >= 0 { '+' } else { '-' };
    let exp_abs = exp.unsigned_abs();
    let exp_str = if exp_abs < 10 {
        format!("0{}", exp_abs)
    } else {
        format!("{}", exp_abs)
    };

    let prefix = if negative { "-" } else { "" };
    format!("{}{}{}{}{}", prefix, mantissa_str, e, sign_char, exp_str)
}

/// Format using %g / %G rules: use %f if exponent in [-4, prec), else %e.
/// Strip trailing zeros from fractional part.
fn format_general(val: f64, prec: usize, upper: bool) -> String {
    let prec = if prec == 0 { 1 } else { prec };

    if val == 0.0 {
        return "0".to_string();
    }

    let abs_val = val.abs();
    let exp = abs_val.log10().floor() as i32;

    if exp >= -4 && exp < prec as i32 {
        // Use %f style with (prec - 1 - exp) decimal places
        let decimal_places = (prec as i32 - 1 - exp).max(0) as usize;
        let formatted = format!("{:.prec$}", val, prec = decimal_places);
        strip_trailing_zeros(&formatted)
    } else {
        // Use %e style with (prec - 1) decimal places
        let sci = format_scientific(val, prec - 1, upper);
        // Strip trailing zeros from the mantissa part (before e/E)
        if let Some(e_pos) = sci.find(if upper { 'E' } else { 'e' }) {
            let (mantissa_part, exp_part) = sci.split_at(e_pos);
            let stripped = strip_trailing_zeros(mantissa_part);
            format!("{}{}", stripped, exp_part)
        } else {
            sci
        }
    }
}

/// Strip trailing zeros after decimal point. Remove the dot if no digits remain.
fn strip_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let trimmed = s.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    trimmed.to_string()
}

// ---------------------------------------------------------------------------
// Character formatting (%c)
// ---------------------------------------------------------------------------

fn format_char(spec: &FormatSpec, arg: &str, out: &mut dyn Write) {
    let ch = if arg.is_empty() {
        String::new()
    } else {
        arg.chars().next().unwrap().to_string()
    };
    pad_and_write(&ch, spec.width, spec.left_align, ' ', out);
}

// ---------------------------------------------------------------------------
// Shell quoting (%q)
// ---------------------------------------------------------------------------

fn format_shell_quote(arg: &str, out: &mut dyn Write) {
    if arg.is_empty() {
        let _ = out.write_all(b"''");
        return;
    }

    // Check if arg contains only safe chars
    let safe = arg.bytes().all(|b| {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'/' || b == b'.' || b == b'-' || b == b':'
    });

    if safe {
        let _ = out.write_all(arg.as_bytes());
    } else {
        // Use $'...' quoting with backslash escapes
        let _ = out.write_all(b"$'");
        for ch in arg.chars() {
            match ch {
                '\'' => {
                    let _ = out.write_all(b"\\'");
                }
                '\\' => {
                    let _ = out.write_all(b"\\\\");
                }
                '\n' => {
                    let _ = out.write_all(b"\\n");
                }
                '\t' => {
                    let _ = out.write_all(b"\\t");
                }
                '\r' => {
                    let _ = out.write_all(b"\\r");
                }
                '\x07' => {
                    let _ = out.write_all(b"\\a");
                }
                '\x08' => {
                    let _ = out.write_all(b"\\b");
                }
                '\x0C' => {
                    let _ = out.write_all(b"\\f");
                }
                '\x0B' => {
                    let _ = out.write_all(b"\\v");
                }
                c if c.is_ascii_graphic() || c == ' ' => {
                    let mut buf = [0u8; 4];
                    let _ = out.write_all(c.encode_utf8(&mut buf).as_bytes());
                }
                c => {
                    // Hex escape for non-printable
                    let _ = write!(out, "\\x{:02x}", c as u32);
                }
            }
        }
        let _ = out.write_all(b"'");
    }
}

// ---------------------------------------------------------------------------
// %b — interpret escapes in argument
// ---------------------------------------------------------------------------

/// Format with %b: interpret backslash escapes in the argument value.
/// Returns `true` if `\c` was encountered (early stop signal).
fn format_backslash_b(arg: &str, out: &mut dyn Write) -> bool {
    !interpret_escapes(arg, out)
}

// ---------------------------------------------------------------------------
// %(strftime)T
// ---------------------------------------------------------------------------

/// Handle `%(...)T` starting at the `(` after `%`.
/// Returns (status, chars_consumed).
fn handle_strftime(
    chars: &[char],
    args: &[String],
    arg_idx: &mut usize,
    out: &mut dyn Write,
) -> (i32, usize) {
    debug_assert!(chars[0] == '(');

    // Find closing )T
    let mut i = 1;
    let len = chars.len();
    while i < len {
        if chars[i] == ')' && i + 1 < len && chars[i + 1] == 'T' {
            let datefmt: String = chars[1..i].iter().collect();
            let consumed = i + 2; // past )T

            let arg = get_arg_str(args, arg_idx);
            let st = format_strftime(&datefmt, &arg, out);
            return (st, consumed);
        }
        i += 1;
    }

    // No matching )T found — output literal
    let _ = out.write_all(b"%(");
    (1, 1) // consumed just the '('
}

fn format_strftime(datefmt: &str, arg: &str, out: &mut dyn Write) -> i32 {
    let timestamp = if arg.is_empty() || arg == "-1" || arg == "-2" {
        chrono::Utc::now().timestamp()
    } else {
        let (v, _) = parse_int_arg(arg);
        v
    };

    let dt = match chrono::DateTime::from_timestamp(timestamp, 0) {
        Some(utc) => utc.with_timezone(&chrono::Local),
        None => {
            let _ = out.write_all(b"");
            return 1;
        }
    };

    let formatted = dt.format(datefmt).to_string();
    let _ = out.write_all(formatted.as_bytes());
    0
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(format: &str, args: &[&str]) -> String {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let mut buf = Vec::new();
        printf_format(format, &args, &mut buf);
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn test_basic_string() {
        assert_eq!(fmt("%s", &["hello"]), "hello");
    }

    #[test]
    fn test_basic_int() {
        assert_eq!(fmt("%d", &["42"]), "42");
    }

    #[test]
    fn test_hex() {
        assert_eq!(fmt("%x", &["255"]), "ff");
        assert_eq!(fmt("%X", &["255"]), "FF");
    }

    #[test]
    fn test_octal() {
        assert_eq!(fmt("%o", &["8"]), "10");
    }

    #[test]
    fn test_escape_newline() {
        assert_eq!(fmt("a\\nb", &[]), "a\nb");
    }

    #[test]
    fn test_escape_hex() {
        assert_eq!(fmt("\\x41", &[]), "A");
    }

    #[test]
    fn test_cyclic() {
        assert_eq!(fmt("%s\\n", &["a", "b", "c"]), "a\nb\nc\n");
    }

    #[test]
    fn test_missing_arg_string() {
        assert_eq!(fmt("%s|%s", &["hello"]), "hello|");
    }

    #[test]
    fn test_missing_arg_int() {
        assert_eq!(fmt("%d", &[]), "0");
    }

    #[test]
    fn test_width_string() {
        assert_eq!(fmt("[%10s]", &["hi"]), "[        hi]");
    }

    #[test]
    fn test_left_align() {
        assert_eq!(fmt("[%-10s]", &["hi"]), "[hi        ]");
    }

    #[test]
    fn test_zero_pad() {
        assert_eq!(fmt("[%05d]", &["42"]), "[00042]");
    }

    #[test]
    fn test_precision_string() {
        assert_eq!(fmt("[%.3s]", &["hello"]), "[hel]");
    }

    #[test]
    fn test_precision_int() {
        assert_eq!(fmt("[%6.4d]", &["42"]), "[  0042]");
    }

    #[test]
    fn test_float() {
        assert_eq!(fmt("[%.2f]", &["3.14159"]), "[3.14]");
    }

    #[test]
    fn test_percent_literal() {
        assert_eq!(fmt("%%", &[]), "%");
    }

    #[test]
    fn test_hex_arg() {
        assert_eq!(fmt("%d", &["0xff"]), "255");
    }

    #[test]
    fn test_octal_arg() {
        assert_eq!(fmt("%d", &["077"]), "63");
    }

    #[test]
    fn test_char_arg() {
        assert_eq!(fmt("%d", &["'A"]), "65");
    }

    #[test]
    fn test_hash_hex() {
        assert_eq!(fmt("%#x", &["255"]), "0xff");
    }

    #[test]
    fn test_hash_octal() {
        assert_eq!(fmt("%#o", &["8"]), "010");
    }

    #[test]
    fn test_char_conv() {
        assert_eq!(fmt("%c", &["hello"]), "h");
    }

    #[test]
    fn test_negative_zero_pad() {
        assert_eq!(fmt("[%010d]", &["-42"]), "[-000000042]");
    }

    #[test]
    fn test_shell_quote_safe() {
        assert_eq!(fmt("%q", &["hello"]), "hello");
    }

    #[test]
    fn test_shell_quote_special() {
        let result = fmt("%q", &["hello world"]);
        assert!(result.contains("hello") && result.contains("world"));
        assert_ne!(result, "hello world");
    }

    #[test]
    fn test_backslash_b() {
        assert_eq!(fmt("%b", &["a\\nb"]), "a\nb");
    }

    #[test]
    fn test_unsigned() {
        assert_eq!(fmt("%u", &["42"]), "42");
    }

    #[test]
    fn test_parse_int_hex() {
        assert_eq!(parse_int_arg("0xff"), (255, false));
    }

    #[test]
    fn test_parse_int_octal() {
        assert_eq!(parse_int_arg("077"), (63, false));
    }

    #[test]
    fn test_parse_int_char() {
        assert_eq!(parse_int_arg("'A"), (65, false));
    }

    #[test]
    fn test_parse_int_empty() {
        assert_eq!(parse_int_arg(""), (0, false));
    }
}
