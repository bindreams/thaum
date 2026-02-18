//! Tests for the CLI tool's YAML output.
//!
//! These tests run the `shell-parse` binary and verify the output format
//! is correct. They catch formatting regressions like duplicate keys,
//! empty lines, wrong source locations, YAML tags, etc.

#![cfg(feature = "cli")]

use std::process::Command;

/// Run shell-parse on the given input and return stdout.
fn run(input: &str) -> String {
    let bin = env!("CARGO_BIN_EXE_shell-parse");
    let output = Command::new(bin)
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .expect("failed to run shell-parse");

    assert!(
        output.status.success(),
        "shell-parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("non-utf8 output")
}

/// Assert that the output contains no empty YAML list items.
fn assert_no_empty_list_items(output: &str) {
    let lines: Vec<&str> = output.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_end();
        if trimmed.ends_with("- ") || trimmed == "-" {
            panic!(
                "Empty list item at line {} (followed by line {}): {:?}",
                i + 1,
                lines.get(i + 1).unwrap_or(&"<EOF>"),
                trimmed
            );
        }
    }
}

/// Assert that the output uses `type: X` instead of YAML tags like `!X`.
fn assert_no_yaml_tags(output: &str) {
    for (i, line) in output.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("- !") || trimmed.starts_with("!") {
            panic!(
                "YAML tag found at line {} — use 'type: X' instead: {:?}",
                i + 1,
                trimmed
            );
        }
    }
}

/// Assert that no YAML mapping has duplicate keys at the same indent level.
fn assert_no_duplicate_keys(output: &str) {
    let lines: Vec<&str> = output.lines().collect();
    for i in 0..lines.len().saturating_sub(1) {
        let line = lines[i];
        let next = lines[i + 1];
        if let (Some(key1), Some(key2)) = (extract_key(line), extract_key(next)) {
            let indent1 = line.len() - line.trim_start().len();
            let indent2 = next.len() - next.trim_start().len();
            if indent1 == indent2 && key1 == key2 {
                panic!(
                    "Duplicate key '{}' at lines {} and {}: {:?} / {:?}",
                    key1,
                    i + 1,
                    i + 2,
                    line,
                    next
                );
            }
        }
    }
}

fn extract_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let content = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    if let Some(colon) = content.find(": ") {
        Some(&content[..colon])
    } else if content.ends_with(':') {
        Some(&content[..content.len() - 1])
    } else {
        None
    }
}

/// Run all standard assertions on CLI output.
fn assert_valid_output(output: &str) {
    assert_no_empty_list_items(output);
    assert_no_duplicate_keys(output);
    assert_no_yaml_tags(output);
}

// ============================================================
// Simple command output
// ============================================================

#[test]
fn cli_simple_command() {
    let output = run("echo hello");
    assert_valid_output(&output);
    assert!(output.contains("type: Command"));
    assert!(output.contains("- echo"));
    assert!(output.contains("- hello"));
}

// ============================================================
// Pipeline output
// ============================================================

#[test]
fn cli_pipeline_no_duplicate_source() {
    let output = run("echo hello | grep h");
    assert_valid_output(&output);
    assert!(output.contains("type: Pipe"));
    assert!(output.contains("type: Command"));
}

// ============================================================
// Command substitution
// ============================================================

#[test]
fn cli_command_substitution_formatting() {
    let output = run("echo $(echo test)");
    assert_valid_output(&output);
    assert!(output.contains("statements:"));
    // The inner statements should not have source annotations
    // (inner parse produces relative offsets)
    let in_subst = output
        .split("type: CommandSubstitution")
        .nth(1)
        .expect("should contain CommandSubstitution");
    assert!(
        !in_subst.contains("source:"),
        "command substitution inner statements should not have source annotations, got: {}",
        in_subst
    );
}

#[test]
fn cli_command_substitution_with_pipeline() {
    let output = run("echo $(ls | grep foo)");
    assert_valid_output(&output);
    assert!(output.contains("type: Pipe"));
    assert!(output.contains("statements:"));
}

#[test]
fn cli_command_substitution_with_semicolon() {
    let output = run("echo hello | grep $(echo test) ;");
    assert_valid_output(&output);
    assert!(output.contains("mode: Terminated"));
}

// ============================================================
// Background & execution modes
// ============================================================

#[test]
fn cli_background_mode() {
    let output = run("cmd &");
    assert_valid_output(&output);
    assert!(output.contains("mode: Background"));
}

#[test]
fn cli_terminated_mode() {
    let output = run("a; b");
    assert_valid_output(&output);
    assert!(output.contains("mode: Terminated"));
}

// ============================================================
// Complex expressions
// ============================================================

#[test]
fn cli_and_or_pipe() {
    let output = run("a | b && c || d");
    assert_valid_output(&output);
    assert!(output.contains("type: Or"));
    assert!(output.contains("type: And"));
    assert!(output.contains("type: Pipe"));
}

#[test]
fn cli_compound_command() {
    let output = run("if true; then echo yes; fi");
    assert_valid_output(&output);
    assert!(output.contains("type: IfClause"));
}

// ============================================================
// Word parts — should use type: instead of YAML tags
// ============================================================

#[test]
fn cli_word_parts_no_tags() {
    let output = run(r#"echo "hello $name" '${x}' *.txt ~/bin $((1+2))"#);
    assert_valid_output(&output);
}

#[test]
fn cli_redirects_no_tags() {
    let output = run("cmd < input > output 2>&1 >> log");
    assert_valid_output(&output);
}

// ============================================================
// Error output — compiler-style diagnostics
// ============================================================

/// Run shell-parse on invalid input and return stderr.
fn run_err(input: &str) -> String {
    let bin = env!("CARGO_BIN_EXE_shell-parse");
    let output = Command::new(bin)
        .arg("-")
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .expect("failed to run shell-parse");

    assert!(
        !output.status.success(),
        "expected shell-parse to fail, but it succeeded"
    );
    String::from_utf8(output.stderr).expect("non-utf8 stderr")
}

#[test]
fn cli_error_shows_error_label() {
    let err = run_err("if true; then fi");
    assert!(
        err.contains("error:"),
        "should start with 'error:': {}",
        err
    );
}

#[test]
fn cli_error_shows_source_location() {
    let err = run_err("if true; then fi");
    assert!(
        err.contains("-->"),
        "should contain ' --> ' location arrow: {}",
        err
    );
    assert!(
        err.contains("<stdin>:"),
        "should reference the filename: {}",
        err
    );
}

#[test]
fn cli_error_shows_source_line() {
    let err = run_err("if true; then fi");
    // Should display the actual source code line
    assert!(
        err.contains("if true; then fi"),
        "should show the source line: {}",
        err
    );
}

#[test]
fn cli_error_shows_underline() {
    let err = run_err("if true; then fi");
    // Should have carets/underline pointing at the error
    assert!(err.contains('^'), "should contain '^' underline: {}", err);
}

#[test]
fn cli_error_unterminated_subst() {
    let err = run_err("echo $(test");
    assert!(err.contains("error:"));
    assert!(err.contains("-->"));
    assert!(err.contains("echo $(test"));
    // Should say "unterminated", not "unexpected character ')'"
    assert!(
        !err.contains("unexpected character"),
        "unterminated $( should not say 'unexpected character': {}",
        err
    );
}

#[test]
fn cli_error_no_internal_names() {
    // Error messages should use shell syntax, not Rust debug names
    let err = run_err("if true; then fi");
    // Should say something like 'fi' or `fi`, not "Fi"
    assert!(
        !err.contains(" Fi"),
        "should not leak internal token name 'Fi': {}",
        err
    );
}

#[test]
fn cli_error_no_debug_token_names() {
    let err = run_err("if true; then done");
    assert!(
        !err.contains(" Done"),
        "should not leak internal token name 'Done': {}",
        err
    );
}
