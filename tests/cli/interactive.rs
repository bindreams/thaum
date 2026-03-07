//! PTY-based integration tests for interactive mode.
//!
//! These tests spawn the thaum binary in a pseudo-terminal via `expectrl` and
//! verify real interactive behavior: prompts, continuation, signals.

use std::time::Duration;

use expectrl::Expect;

fn spawn_thaum_interactive() -> impl Expect {
    let bin = env!("CARGO_BIN_EXE_thaum");
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("exec");
    cmd.env("PS1", "$ ");
    cmd.env("PS2", "> ");
    cmd.env("NO_COLOR", "1");
    cmd.env("HOME", "/tmp/claude/thaum-test-home");
    let mut session = expectrl::Session::spawn(cmd).expect("failed to spawn thaum");
    session.set_expect_timeout(Some(Duration::from_secs(5)));
    session
}

#[skuld::test(labels = [interactive])]
fn repl_echo_hello() {
    let mut session = spawn_thaum_interactive();
    session.expect("$ ").expect("expected initial prompt");
    session.send_line("echo hello").expect("send failed");
    session.expect("hello").expect("expected 'hello' output");
    session.send_line("exit").expect("send exit failed");
}

#[skuld::test(labels = [interactive])]
fn repl_variable_persists_across_lines() {
    let mut session = spawn_thaum_interactive();
    session.expect("$ ").expect("expected initial prompt");
    session.send_line("X=world").expect("send failed");
    session.expect("$ ").expect("expected prompt after assignment");
    session.send_line("echo $X").expect("send failed");
    session.expect("world").expect("expected 'world' output");
    session.send_line("exit").expect("send exit failed");
}

#[skuld::test(labels = [interactive])]
fn repl_syntax_error_continues() {
    let mut session = spawn_thaum_interactive();
    session.expect("$ ").expect("expected initial prompt");
    session.send_line("if true; then fi").expect("send failed");
    // Should get an error but not exit
    session.expect("$ ").expect("expected re-prompt after syntax error");
    session.send_line("echo ok").expect("send failed");
    session.expect("ok").expect("expected 'ok' after recovery");
    session.send_line("exit").expect("send exit failed");
}

#[skuld::test(labels = [interactive])]
fn repl_piped_stdin_no_interactive() {
    // When stdin is piped (not a TTY), interactive mode should NOT activate.
    let bin = env!("CARGO_BIN_EXE_thaum");
    let output = std::process::Command::new(bin)
        .args(["exec", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.as_mut().unwrap().write_all(b"echo hello\n").unwrap();
            child.wait_with_output()
        })
        .expect("failed to run thaum");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), "hello");
    assert!(output.status.success());
}
