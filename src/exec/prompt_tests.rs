//! Unit tests for Bash prompt escape sequence expansion.

use crate::exec::prompt::{expand_prompt_escapes, PromptContext};
use crate::Dialect;

fn bash_ctx() -> PromptContext {
    PromptContext {
        username: "testuser".into(),
        hostname: "myhost.example.com".into(),
        cwd: "/home/testuser/src".into(),
        home: "/home/testuser".into(),
        shell_name: "thaum".into(),
        version: "0.1".into(),
        version_patch: "0.1.0".into(),
        uid: 1000,
        history_number: 42,
        command_number: 7,
        jobs_count: 0,
        tty_name: "pts/3".into(),
    }
}

fn posix_ctx() -> PromptContext {
    bash_ctx()
}

// Bash mode ===========================================================================================================

#[skuld::test]
fn escape_backslash_u_expands_to_username() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\u", &ctx, &Dialect::Bash.options()), "testuser");
}

#[skuld::test]
fn escape_backslash_h_short_hostname() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\h", &ctx, &Dialect::Bash.options()), "myhost");
}

#[skuld::test]
fn escape_backslash_big_h_full_hostname() {
    let ctx = bash_ctx();
    assert_eq!(
        expand_prompt_escapes(r"\H", &ctx, &Dialect::Bash.options()),
        "myhost.example.com"
    );
}

#[skuld::test]
fn escape_backslash_w_cwd_with_tilde() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\w", &ctx, &Dialect::Bash.options()), "~/src");
}

#[skuld::test]
fn escape_backslash_big_w_basename_only() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\W", &ctx, &Dialect::Bash.options()), "src");
}

#[skuld::test]
fn escape_backslash_w_at_home() {
    let mut ctx = bash_ctx();
    ctx.cwd = "/home/testuser".into();
    assert_eq!(expand_prompt_escapes(r"\w", &ctx, &Dialect::Bash.options()), "~");
}

#[skuld::test]
fn escape_backslash_big_w_at_home() {
    let mut ctx = bash_ctx();
    ctx.cwd = "/home/testuser".into();
    assert_eq!(expand_prompt_escapes(r"\W", &ctx, &Dialect::Bash.options()), "~");
}

#[skuld::test]
fn escape_backslash_dollar_user() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\$", &ctx, &Dialect::Bash.options()), "$");
}

#[skuld::test]
fn escape_backslash_dollar_root() {
    let mut ctx = bash_ctx();
    ctx.uid = 0;
    assert_eq!(expand_prompt_escapes(r"\$", &ctx, &Dialect::Bash.options()), "#");
}

#[skuld::test]
fn escape_newline_and_return() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\n", &ctx, &Dialect::Bash.options()), "\n");
    assert_eq!(expand_prompt_escapes(r"\r", &ctx, &Dialect::Bash.options()), "\r");
}

#[skuld::test]
fn escape_bell_and_escape() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\a", &ctx, &Dialect::Bash.options()), "\x07");
    assert_eq!(expand_prompt_escapes(r"\e", &ctx, &Dialect::Bash.options()), "\x1b");
}

#[skuld::test]
fn escape_backslash_backslash() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\\", &ctx, &Dialect::Bash.options()), "\\");
}

#[skuld::test]
fn escape_brackets_become_readline_markers() {
    let ctx = bash_ctx();
    let result = expand_prompt_escapes(r"\[color\]", &ctx, &Dialect::Bash.options());
    assert_eq!(result, "\x01color\x02");
}

#[skuld::test]
fn escape_octal() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\101", &ctx, &Dialect::Bash.options()), "A");
    // octal 101 = 65 = 'A'
}

#[skuld::test]
fn escape_hex() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\x41", &ctx, &Dialect::Bash.options()), "A");
    // hex 41 = 65 = 'A'
}

#[skuld::test]
fn escape_s_shell_name() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\s", &ctx, &Dialect::Bash.options()), "thaum");
}

#[skuld::test]
fn escape_v_version() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\v", &ctx, &Dialect::Bash.options()), "0.1");
}

#[skuld::test]
fn escape_big_v_version_patch() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\V", &ctx, &Dialect::Bash.options()), "0.1.0");
}

#[skuld::test]
fn escape_bang_history_number() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\!", &ctx, &Dialect::Bash.options()), "42");
}

#[skuld::test]
fn escape_hash_command_number() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\#", &ctx, &Dialect::Bash.options()), "7");
}

#[skuld::test]
fn escape_j_jobs() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\j", &ctx, &Dialect::Bash.options()), "0");
}

#[skuld::test]
fn escape_l_tty_basename() {
    let ctx = bash_ctx();
    assert_eq!(expand_prompt_escapes(r"\l", &ctx, &Dialect::Bash.options()), "3");
}

#[skuld::test]
fn mixed_escapes_and_literal() {
    let ctx = bash_ctx();
    let result = expand_prompt_escapes(r"\u@\h:\w\$ ", &ctx, &Dialect::Bash.options());
    assert_eq!(result, "testuser@myhost:~/src$ ");
}

// POSIX mode ==========================================================================================================

#[skuld::test]
fn posix_mode_no_escape_sequences() {
    let ctx = posix_ctx();
    // POSIX doesn't define prompt escape sequences — backslashes are literal.
    assert_eq!(expand_prompt_escapes(r"\u", &ctx, &Dialect::Posix.options()), r"\u");
}

#[skuld::test]
fn posix_mode_literal_passthrough() {
    let ctx = posix_ctx();
    assert_eq!(expand_prompt_escapes("$ ", &ctx, &Dialect::Posix.options()), "$ ");
}
