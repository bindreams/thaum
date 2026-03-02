//! Printf builtin formatter.
//!
//! Implements the full `printf FORMAT [ARGUMENTS...]` semantics matching
//! bash behaviour: custom formatting (not delegating to Rust's `format!`),
//! cyclic argument reuse, backslash escapes, `%b`, `%q`, `%(...)T`, numeric
//! argument parsing (hex, octal, char-code), and the `-v VAR` option.

use std::io::Write;

// Public entry point ==================================================================================================

/// Format `fmt` with `args` into `out`, cycling through the format string
/// until all arguments are consumed. Returns 0 on success, 1 on any error.
///
/// `decimal_sep` is the LC_NUMERIC decimal separator character used for
/// float output (%f, %e, %g) and float input parsing.
pub fn printf_format(
    fmt: &str,
    args: &[String],
    out: &mut dyn Write,
    decimal_sep: char,
    env: &crate::exec::Environment,
) -> i32 {
    let mut status = 0;
    let mut arg_idx: usize = 0;
    let args_len = args.len();

    loop {
        let start_idx = arg_idx;
        let (st, stop) = format_once(fmt, args, &mut arg_idx, out, decimal_sep, env);
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
    decimal_sep: char,
    env: &crate::exec::Environment,
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
                    let (st, advance) = handle_strftime(&chars[i..], args, arg_idx, env, out);
                    if st != 0 {
                        status = st;
                    }
                    i += advance;
                    continue;
                }
                let (spec, advance) = parse_format_spec(&chars[i..], args, arg_idx);
                i += advance;
                let st = apply_format_spec(&spec, args, arg_idx, out, decimal_sep);
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

// Format spec =========================================================================================================

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

// Apply a format spec =================================================================================================

fn apply_format_spec(
    spec: &FormatSpec,
    args: &[String],
    arg_idx: &mut usize,
    out: &mut dyn Write,
    decimal_sep: char,
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
            format_float(spec, &arg, spec.conversion, out, decimal_sep)
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

// Argument access =====================================================================================================

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

// Numeric argument parsing ============================================================================================

/// Parse an argument as an integer, handling hex (0x), octal (leading 0),
/// character code ('A), signs, and leading whitespace.
/// Returns (value, had_error).
fn parse_int_arg(s: &str) -> (i64, bool) {
    match super::numeric::parse_shell_int(s) {
        Ok(v) => (v, false),
        Err(()) => (0, true),
    }
}

/// Parse an argument as a float. Same prefix handling as int but falls
/// through to f64 parsing.
///
/// `decimal_sep` is the locale decimal separator; if not `'.'`, occurrences
/// in `s` are replaced with `'.'` before Rust's `f64::parse`.
fn parse_float_arg(s: &str, decimal_sep: char) -> (f64, bool) {
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

    // Normalize locale decimal separator to '.' for Rust's parser
    let normalized = if decimal_sep != '.' {
        std::borrow::Cow::Owned(s.replace(decimal_sep, "."))
    } else {
        std::borrow::Cow::Borrowed(s)
    };

    match normalized.parse::<f64>() {
        Ok(v) => (v, false),
        Err(_) => (0.0, true),
    }
}

// Escape interpretation (shared by format string and %b) ==============================================================

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

// Padding helper ======================================================================================================

fn pad_and_write(content: &str, width: Option<usize>, left_align: bool, pad_char: char, out: &mut dyn Write) {
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

// String formatting (%s) ==============================================================================================

fn format_string(spec: &FormatSpec, arg: &str, out: &mut dyn Write) {
    let truncated = match spec.precision {
        Some(p) if p < arg.len() => &arg[..p],
        _ => arg,
    };
    pad_and_write(truncated, spec.width, spec.left_align, ' ', out);
}

// Shared integer formatting core ======================================================================================

/// Shared formatting for integer-family specifiers (%d, %u, %x, %o).
///
/// Takes pre-computed `prefix` (sign or radix marker) and `digits` (absolute
/// value in the appropriate radix), applies precision zero-padding and width
/// padding, and writes the result.
fn format_int_core(prefix: &str, digits: &str, spec: &FormatSpec, out: &mut dyn Write) {
    // Precision: minimum number of digits (zero-padded)
    let padded_digits = match spec.precision {
        Some(p) if p > digits.len() => format!("{}{}", "0".repeat(p - digits.len()), digits),
        _ => digits.to_string(),
    };

    let total = prefix.len() + padded_digits.len();
    let width = spec.width.unwrap_or(0);

    if spec.zero_pad && !spec.left_align && spec.precision.is_none() && total < width {
        // Zero-pad between prefix and digits
        let zeros = "0".repeat(width - total);
        let content = format!("{prefix}{zeros}{padded_digits}");
        let _ = out.write_all(content.as_bytes());
    } else {
        let content = format!("{prefix}{padded_digits}");
        pad_and_write(&content, spec.width, spec.left_align, ' ', out);
    }
}

// Signed integer formatting (%d, %i) ==================================================================================

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

    let sign = if is_negative {
        "-"
    } else if spec.plus_sign {
        "+"
    } else if spec.space_sign {
        " "
    } else {
        ""
    };

    format_int_core(sign, &abs_str, spec, out);
    status
}

// Unsigned integer formatting (%u) ====================================================================================

fn format_unsigned_int(spec: &FormatSpec, arg: &str, out: &mut dyn Write) -> i32 {
    let (val, had_error) = parse_int_arg(arg);
    let status = if had_error { 1 } else { 0 };

    let uval = val as u64;
    let digits = uval.to_string();

    format_int_core("", &digits, spec, out);
    status
}

// Hex formatting (%x, %X) =============================================================================================

fn format_hex(spec: &FormatSpec, arg: &str, uppercase: bool, out: &mut dyn Write) -> i32 {
    let (val, had_error) = parse_int_arg(arg);
    let status = if had_error { 1 } else { 0 };

    let uval = val as u64;
    let digits = if uppercase {
        format!("{uval:X}")
    } else {
        format!("{uval:x}")
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

    format_int_core(prefix, &digits, spec, out);
    status
}

// Octal formatting (%o) ===============================================================================================

fn format_octal(spec: &FormatSpec, arg: &str, out: &mut dyn Write) -> i32 {
    let (val, had_error) = parse_int_arg(arg);
    let status = if had_error { 1 } else { 0 };

    let uval = val as u64;
    let digits = format!("{uval:o}");

    // # prefix for octal is "0" (not "0o" like Rust!).
    // Precision zero-padding guarantees a leading '0', so the prefix is only
    // needed when digits don't already start with '0' AND precision won't pad.
    let needs_hash_prefix = spec.hash && !digits.starts_with('0') && spec.precision.is_none_or(|p| p <= digits.len());
    let prefix = if needs_hash_prefix { "0" } else { "" };

    format_int_core(prefix, &digits, spec, out);
    status
}

// Float formatting (%f, %e, %E, %g, %G) ===============================================================================

fn format_float(spec: &FormatSpec, arg: &str, conv: char, out: &mut dyn Write, decimal_sep: char) -> i32 {
    let (val, had_error) = parse_float_arg(arg, decimal_sep);
    let status = if had_error { 1 } else { 0 };

    let prec = spec.precision.unwrap_or(6);

    let formatted = match conv {
        'f' => format!("{val:.prec$}"),
        'e' => format_scientific(val, prec, false),
        'E' => format_scientific(val, prec, true),
        'g' => format_general(val, prec, false),
        'G' => format_general(val, prec, true),
        _ => unreachable!(),
    };

    // Replace Rust's '.' with the locale decimal separator
    let formatted = if decimal_sep != '.' {
        formatted.replace('.', &decimal_sep.to_string())
    } else {
        formatted
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

    let content = format!("{sign}{formatted}");
    let total = content.len();
    let width = spec.width.unwrap_or(0);

    if spec.zero_pad && !spec.left_align && total < width {
        // Zero-pad between sign and digits
        if content.starts_with('-') || content.starts_with('+') || content.starts_with(' ') {
            let (s, rest) = content.split_at(1);
            let zeros = "0".repeat(width - total);
            let padded = format!("{s}{zeros}{rest}");
            let _ = out.write_all(padded.as_bytes());
        } else {
            let zeros = "0".repeat(width - total);
            let padded = format!("{zeros}{content}");
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
        let frac = if prec > 0 { format!(".{zeros}") } else { String::new() };
        let e = if upper { 'E' } else { 'e' };
        return format!("0{frac}{e}+00");
    }

    let negative = val.is_sign_negative();
    let abs_val = val.abs();
    let exp = abs_val.log10().floor() as i32;
    let mantissa = abs_val / 10f64.powi(exp);

    // Format mantissa with given precision
    let mantissa_str = format!("{mantissa:.prec$}");

    let e = if upper { 'E' } else { 'e' };
    let sign_char = if exp >= 0 { '+' } else { '-' };
    let exp_abs = exp.unsigned_abs();
    let exp_str = if exp_abs < 10 {
        format!("0{exp_abs}")
    } else {
        format!("{exp_abs}")
    };

    let prefix = if negative { "-" } else { "" };
    format!("{prefix}{mantissa_str}{e}{sign_char}{exp_str}")
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
        let formatted = format!("{val:.decimal_places$}");
        strip_trailing_zeros(&formatted)
    } else {
        // Use %e style with (prec - 1) decimal places
        let sci = format_scientific(val, prec - 1, upper);
        // Strip trailing zeros from the mantissa part (before e/E)
        if let Some(e_pos) = sci.find(if upper { 'E' } else { 'e' }) {
            let (mantissa_part, exp_part) = sci.split_at(e_pos);
            let stripped = strip_trailing_zeros(mantissa_part);
            format!("{stripped}{exp_part}")
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

// Character formatting (%c) ===========================================================================================

fn format_char(spec: &FormatSpec, arg: &str, out: &mut dyn Write) {
    let ch = if arg.is_empty() {
        String::new()
    } else {
        arg.chars().next().unwrap().to_string()
    };
    pad_and_write(&ch, spec.width, spec.left_align, ' ', out);
}

// Shell quoting (%q) ==================================================================================================

fn format_shell_quote(arg: &str, out: &mut dyn Write) {
    if arg.is_empty() {
        let _ = out.write_all(b"''");
        return;
    }

    // Check if arg contains only safe chars
    let safe = arg
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'/' || b == b'.' || b == b'-' || b == b':');

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

// %b -- interpret escapes in argument =================================================================================

/// Format with %b: interpret backslash escapes in the argument value.
/// Returns `true` if `\c` was encountered (early stop signal).
fn format_backslash_b(arg: &str, out: &mut dyn Write) -> bool {
    !interpret_escapes(arg, out)
}

// %(strftime)T ========================================================================================================

/// Handle `%(...)T` starting at the `(` after `%`.
/// Returns (status, chars_consumed).
fn handle_strftime(
    chars: &[char],
    args: &[String],
    arg_idx: &mut usize,
    env: &crate::exec::Environment,
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
            let st = format_strftime(&datefmt, &arg, env, out);
            return (st, consumed);
        }
        i += 1;
    }

    // No matching )T found — output literal
    let _ = out.write_all(b"%(");
    (1, 1) // consumed just the '('
}

fn format_strftime(datefmt: &str, arg: &str, env: &crate::exec::Environment, out: &mut dyn Write) -> i32 {
    let timestamp = if arg.is_empty() || arg == "-1" || arg == "-2" {
        jiff::Timestamp::now().as_second()
    } else {
        let (v, _) = parse_int_arg(arg);
        v
    };

    let formatted = super::locale::strftime_locale(datefmt, timestamp, env);
    let _ = out.write_all(formatted.as_bytes());
    0
}

#[cfg(test)]
#[path = "printf_tests.rs"]
mod tests;
