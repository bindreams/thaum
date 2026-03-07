//! Prompt escape sequence expansion for Bash's PS1/PS2/PS4.
//!
//! POSIX defines no prompt escape sequences — only parameter/command expansion
//! is applied (handled separately by the caller). Bash adds `\u`, `\h`, `\w`,
//! etc. This module implements the first pass: expanding Bash escape sequences
//! into literal text. The second pass (shell variable/command expansion) is the
//! caller's responsibility.

use crate::ShellOptions;

/// Context needed for prompt escape expansion. Collected once per prompt
/// display from the executor's environment and system state.
pub struct PromptContext {
    pub username: String,
    pub hostname: String,
    pub cwd: String,
    pub home: String,
    pub shell_name: String,
    pub version: String,
    pub version_patch: String,
    pub uid: u32,
    pub history_number: usize,
    pub command_number: usize,
    pub jobs_count: usize,
    pub tty_name: String,
}

/// Expand Bash prompt escape sequences in `template`.
///
/// In POSIX mode, returns the template unchanged (POSIX specifies no backslash
/// escapes for prompts). In Bash mode, expands `\u`, `\h`, `\w`, `\$`, etc.
pub fn expand_prompt_escapes(template: &str, ctx: &PromptContext, options: &ShellOptions) -> String {
    if !options.bash_prompt_escapes {
        return template.to_string();
    }

    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }

        match chars.next() {
            None => {
                result.push('\\');
            }
            Some('a') => result.push('\x07'),
            Some('e') => result.push('\x1b'),
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('\\') => result.push('\\'),
            Some('[') => result.push('\x01'),
            Some(']') => result.push('\x02'),

            Some('u') => result.push_str(&ctx.username),
            Some('h') => {
                let short = ctx.hostname.split('.').next().unwrap_or(&ctx.hostname);
                result.push_str(short);
            }
            Some('H') => result.push_str(&ctx.hostname),

            Some('w') => result.push_str(&tilde_abbreviate(&ctx.cwd, &ctx.home)),
            Some('W') => {
                let abbreviated = tilde_abbreviate(&ctx.cwd, &ctx.home);
                if abbreviated == "~" {
                    result.push('~');
                } else {
                    let basename = abbreviated.rsplit('/').next().unwrap_or(&abbreviated);
                    result.push_str(basename);
                }
            }

            Some('$') => {
                if ctx.uid == 0 {
                    result.push('#');
                } else {
                    result.push('$');
                }
            }

            Some('s') => result.push_str(&ctx.shell_name),
            Some('v') => result.push_str(&ctx.version),
            Some('V') => result.push_str(&ctx.version_patch),
            Some('!') => result.push_str(&ctx.history_number.to_string()),
            Some('#') => result.push_str(&ctx.command_number.to_string()),
            Some('j') => result.push_str(&ctx.jobs_count.to_string()),

            Some('l') => {
                let basename = ctx.tty_name.rsplit('/').next().unwrap_or(&ctx.tty_name);
                result.push_str(basename);
            }

            Some('d') => {
                let now = jiff::Zoned::now();
                result.push_str(&now.strftime("%a %b %d").to_string());
            }
            Some('t') => {
                let now = jiff::Zoned::now();
                result.push_str(&now.strftime("%H:%M:%S").to_string());
            }
            Some('T') => {
                let now = jiff::Zoned::now();
                result.push_str(&now.strftime("%I:%M:%S").to_string());
            }
            Some('@') => {
                let now = jiff::Zoned::now();
                result.push_str(&now.strftime("%I:%M %p").to_string());
            }
            Some('A') => {
                let now = jiff::Zoned::now();
                result.push_str(&now.strftime("%H:%M").to_string());
            }

            Some(c @ '0'..='7') => {
                // Octal escape: \nnn (up to 3 octal digits)
                let mut octal = String::new();
                octal.push(c);
                for _ in 0..2 {
                    if let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() && d < '8' {
                            octal.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                if let Ok(val) = u8::from_str_radix(&octal, 8) {
                    result.push(val as char);
                }
            }

            Some('x') => {
                // Hex escape: \xHH (up to 2 hex digits)
                let mut hex = String::new();
                for _ in 0..2 {
                    if let Some(&d) = chars.peek() {
                        if d.is_ascii_hexdigit() {
                            hex.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                if !hex.is_empty() {
                    if let Ok(val) = u8::from_str_radix(&hex, 16) {
                        result.push(val as char);
                    }
                }
            }

            Some(other) => {
                // Unknown escape — preserve as-is
                result.push('\\');
                result.push(other);
            }
        }
    }

    result
}

/// Replace a leading `$HOME` prefix with `~`.
fn tilde_abbreviate(cwd: &str, home: &str) -> String {
    if home.is_empty() {
        return cwd.to_string();
    }
    if cwd == home {
        return "~".to_string();
    }
    if let Some(rest) = cwd.strip_prefix(home) {
        if rest.starts_with('/') {
            return format!("~{rest}");
        }
    }
    cwd.to_string()
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;
