//! Minimal `echo` — prints arguments to stdout. Cross-platform test tool.
//!
//! Supports `-n` (no trailing newline) and `-e` (interpret escape sequences).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut trailing_newline = true;
    let mut interpret_escapes = false;
    let mut start = 0;

    // Parse leading flags (stop at first non-flag argument or "--").
    for (i, arg) in args.iter().enumerate() {
        if arg == "--" {
            start = i + 1;
            break;
        }
        if !arg.starts_with('-') || arg.len() < 2 {
            start = i;
            break;
        }
        let flags = &arg[1..];
        if flags.bytes().all(|c| matches!(c, b'n' | b'e' | b'E')) {
            for c in flags.bytes() {
                match c {
                    b'n' => trailing_newline = false,
                    b'e' => interpret_escapes = true,
                    b'E' => interpret_escapes = false,
                    _ => unreachable!(),
                }
            }
            start = i + 1;
        } else {
            start = i;
            break;
        }
    }

    let text = args[start..].join(" ");

    if interpret_escapes {
        print!("{}", expand_escapes(&text));
    } else {
        print!("{text}");
    }

    if trailing_newline {
        println!();
    }
}

fn expand_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 1;
            match bytes[i] {
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'\\' => out.push('\\'),
                b'a' => out.push('\x07'),
                b'b' => out.push('\x08'),
                b'f' => out.push('\x0c'),
                b'r' => out.push('\r'),
                b'v' => out.push('\x0b'),
                b'0' => {
                    // Octal: up to 3 digits after the '0'.
                    let mut val: u8 = 0;
                    let mut count = 0;
                    while count < 3 && i + 1 < bytes.len() && (b'0'..=b'7').contains(&bytes[i + 1]) {
                        i += 1;
                        val = val.wrapping_mul(8).wrapping_add(bytes[i] - b'0');
                        count += 1;
                    }
                    out.push(val as char);
                }
                b'c' => return out, // \c stops output entirely
                other => {
                    out.push('\\');
                    out.push(other as char);
                }
            }
        } else {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    out
}
