//! Tests for thaum-expect: VT stripping and PTY session.

use std::process::Command;
use std::time::Duration;

use thaum_expect::{ExpectError, PtySession};

fn main() {
    skuld::run_all();
}

// VT stripping ========================================================================================================

#[skuld::test]
fn strip_vt_plain_text_unchanged() {
    assert_eq!(thaum_expect::strip_vt("hello world"), "hello world");
}

#[skuld::test]
fn strip_vt_empty_string() {
    assert_eq!(thaum_expect::strip_vt(""), "");
}

#[skuld::test]
fn strip_vt_csi_sequences() {
    // CSI: ESC [ ... final_byte
    assert_eq!(thaum_expect::strip_vt("\x1b[2J"), ""); // clear screen
    assert_eq!(thaum_expect::strip_vt("\x1b[?25h"), ""); // show cursor
    assert_eq!(thaum_expect::strip_vt("\x1b[1C"), ""); // cursor right
    assert_eq!(thaum_expect::strip_vt("\x1b[H"), ""); // cursor home
    assert_eq!(thaum_expect::strip_vt("\x1b[K"), ""); // erase to end of line
    assert_eq!(thaum_expect::strip_vt("\x1b[0m"), ""); // reset attributes
    assert_eq!(thaum_expect::strip_vt("ab\x1b[2Jcd"), "abcd");
}

#[skuld::test]
fn strip_vt_osc_sequences() {
    // OSC: ESC ] ... BEL
    assert_eq!(thaum_expect::strip_vt("\x1b]0;title\x07"), "");
    assert_eq!(
        thaum_expect::strip_vt("before\x1b]0;window title\x07after"),
        "beforeafter"
    );
}

#[skuld::test]
fn strip_vt_two_byte_escapes() {
    // Two-byte: ESC + single character (not [ or ])
    assert_eq!(thaum_expect::strip_vt("\x1b="), ""); // keypad application mode
    assert_eq!(thaum_expect::strip_vt("\x1b>"), ""); // keypad numeric mode
    assert_eq!(thaum_expect::strip_vt("a\x1b=b"), "ab");
}

#[skuld::test]
fn strip_vt_conpty_prompt_interleaving() {
    // The exact case that breaks expectrl: ConPTY emits the prompt "$ " as
    // "$" + ESC[K + ESC[1C (erase-to-end + cursor-right-by-1 instead of literal space).
    let conpty_prompt = "\x1b[H$\x1b[K\x1b[1C";
    assert_eq!(thaum_expect::strip_vt(conpty_prompt), "$");
    // Note: the space is encoded as cursor-right, so stripped output is just "$".
    // The expect logic must handle this by matching "$" with optional whitespace.
}

#[skuld::test]
fn strip_vt_conpty_init_sequences() {
    // ConPTY sends these on session start.
    let init = "\x1b[?9001h\x1b[?1004h";
    assert_eq!(thaum_expect::strip_vt(init), "");
}

#[skuld::test]
fn strip_vt_mixed_content() {
    let input = "\x1b[?25lhello\x1b[0m world\x1b[?25h";
    assert_eq!(thaum_expect::strip_vt(input), "hello world");
}

// PTY session =========================================================================================================

#[skuld::test]
fn spawn_echo_command() {
    let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "echo" });
    if cfg!(windows) {
        cmd.args(["/C", "echo", "hello from pty"]);
    } else {
        cmd.arg("hello from pty");
    }

    let mut session = PtySession::spawn(cmd).expect("failed to spawn");
    session.set_expect_timeout(Some(Duration::from_secs(5)));

    let m = session.expect("hello from pty").expect("expected output");
    assert!(m.cleaned.contains("hello from pty"));
}

#[skuld::test]
fn expect_timeout() {
    let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "cat" });
    if cfg!(windows) {
        // cmd /C pause will wait for keypress — we want something that blocks
        cmd.args(["/C", "timeout", "/t", "30", "/nobreak"]);
    }
    // On Unix, `cat` with no input will block.

    let mut session = PtySession::spawn(cmd).expect("failed to spawn");
    session.set_expect_timeout(Some(Duration::from_millis(200)));

    match session.expect("this_will_never_appear") {
        Err(ExpectError::Timeout) => {} // expected
        Err(e) => panic!("expected Timeout, got {e:?}"),
        Ok(m) => panic!("expected Timeout, got match: {:?}", m.cleaned),
    }
}
