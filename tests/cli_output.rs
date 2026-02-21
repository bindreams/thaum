//! Tests for the CLI tool's YAML output.
//!
//! These tests run the `thaum` binary and verify the output format
//! is correct. They catch formatting regressions like duplicate keys,
//! empty lines, wrong source locations, YAML tags, etc.

#![cfg(feature = "cli")]

use std::process::Command;

/// Run thaum on the given input and return stdout.
fn run(input: &str) -> String {
    let bin = env!("CARGO_BIN_EXE_thaum");
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
        .expect("failed to run thaum");

    assert!(
        output.status.success(),
        "thaum failed: {}",
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
    } else if let Some(stripped) = content.strip_suffix(':') {
        Some(stripped)
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

/// Run thaum on invalid input and return stderr.
fn run_err(input: &str) -> String {
    let bin = env!("CARGO_BIN_EXE_thaum");
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
        .expect("failed to run thaum");

    assert!(
        !output.status.success(),
        "expected thaum to fail, but it succeeded"
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

// ============================================================
// Exec subcommand
// ============================================================

/// Run thaum exec on the given input and return (stdout, stderr, exit_code).
fn run_exec(input: &str) -> (String, String, i32) {
    run_exec_with_args(&["exec", "-"], input)
}

fn run_exec_with_args(args: &[&str], input: &str) -> (String, String, i32) {
    let bin = env!("CARGO_BIN_EXE_thaum");
    let output = Command::new(bin)
        .args(args)
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
        .expect("failed to run thaum exec");

    let code = output.status.code().unwrap_or(128);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    (stdout, stderr, code)
}

#[test]
fn cli_exec_true() {
    let (_, _, code) = run_exec("true");
    assert_eq!(code, 0);
}

#[test]
fn cli_exec_false() {
    let (_, _, code) = run_exec("false");
    assert_eq!(code, 1);
}

#[test]
fn cli_exec_echo() {
    let (stdout, _, code) = run_exec("echo hello world");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "hello world");
}

#[test]
fn cli_exec_exit_code() {
    let (_, _, code) = run_exec("exit 42");
    assert_eq!(code, 42);
}

#[test]
fn cli_exec_unsupported_feature_error() {
    let (_, stderr, code) = run_exec("echo hello &");
    assert_ne!(code, 0);
    assert!(
        stderr.contains("unsupported feature"),
        "stderr should mention unsupported feature: {}",
        stderr,
    );
}

#[test]
fn cli_exec_parse_error() {
    let (_, stderr, code) = run_exec("if true; then fi");
    assert_eq!(code, 1);
    assert!(
        stderr.contains("error:"),
        "stderr should contain error: {}",
        stderr,
    );
}

#[test]
fn cli_exec_with_bash_flag() {
    let (_, _, code) = run_exec_with_args(&["exec", "--bash", "-"], "true");
    assert_eq!(code, 0);
}

#[test]
fn cli_exec_bash_flag_before_exec() {
    let (_, _, code) = run_exec_with_args(&["--bash", "exec", "-"], "true");
    assert_eq!(code, 0);
}

#[test]
fn cli_exec_variable_and_status() {
    let (stdout, _, code) = run_exec("X=hello; echo $X");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "hello");
}

// ============================================================
// -c / --command flag
// ============================================================

/// Run thaum with given args (no stdin needed) and return (stdout, stderr, exit_code).
fn run_cli(args: &[&str]) -> (String, String, i32) {
    let bin = env!("CARGO_BIN_EXE_thaum");
    let output = Command::new(bin)
        .args(args)
        .env("NO_COLOR", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("failed to run thaum");

    let code = output.status.code().unwrap_or(128);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    (stdout, stderr, code)
}

#[test]
fn cli_parse_c_flag() {
    let (stdout, _, code) = run_cli(&["-c", "echo hello"]);
    assert_eq!(code, 0);
    assert_valid_output(&stdout);
    assert!(stdout.contains("type: Command"));
    assert!(stdout.contains("- echo"));
}

#[test]
fn cli_parse_command_long_flag() {
    let (stdout, _, code) = run_cli(&["--command", "echo hello"]);
    assert_eq!(code, 0);
    assert_valid_output(&stdout);
    assert!(stdout.contains("type: Command"));
}

#[test]
fn cli_parse_subcommand_with_c() {
    let (stdout, _, code) = run_cli(&["parse", "-c", "true"]);
    assert_eq!(code, 0);
    assert_valid_output(&stdout);
    assert!(stdout.contains("type: Command"));
}

#[test]
fn cli_parse_subcommand_with_bash_c() {
    let (stdout, _, code) = run_cli(&["parse", "--bash", "-c", "[[ -n hello ]]"]);
    assert_eq!(code, 0);
    assert_valid_output(&stdout);
    assert!(stdout.contains("type: BashDoubleBracket"));
}

#[test]
fn cli_exec_c_flag() {
    let (stdout, _, code) = run_cli(&["exec", "-c", "echo hello world"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "hello world");
}

#[test]
fn cli_exec_c_with_script_args() {
    let (stdout, _, code) = run_cli(&["exec", "-c", "echo $1 $2", "foo", "bar"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "foo bar");
}

#[test]
fn cli_exec_command_long_flag() {
    let (stdout, _, code) = run_cli(&["exec", "--command", "echo ok"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "ok");
}

// ============================================================
// Explicit parse subcommand
// ============================================================

#[test]
fn cli_parse_subcommand_stdin() {
    let (stdout, _, code) = run_exec_with_args(&["parse", "-"], "echo hello");
    assert_eq!(code, 0);
    assert_valid_output(&stdout);
    assert!(stdout.contains("type: Command"));
    assert!(stdout.contains("- echo"));
}

#[test]
fn cli_parse_subcommand_bash() {
    let (stdout, _, code) = run_exec_with_args(&["parse", "--bash", "-"], "[[ -n x ]]");
    assert_eq!(code, 0);
    assert_valid_output(&stdout);
    assert!(stdout.contains("type: BashDoubleBracket"));
}

// ============================================================
// Lex subcommand
// ============================================================

#[test]
fn cli_lex_simple_command() {
    let (stdout, _, code) = run_cli(&["lex", "-c", "echo hello"]);
    assert_eq!(code, 0);
    // Should have a header
    assert!(stdout.contains("LOCATION"));
    assert!(stdout.contains("TOKEN"));
    assert!(stdout.contains("TEXT"));
    // Should contain the tokens
    assert!(stdout.contains("Literal"));
    assert!(stdout.contains("echo"));
    assert!(stdout.contains("hello"));
}

#[test]
fn cli_lex_operators() {
    let (stdout, _, code) = run_cli(&["lex", "-c", "a && b || c | d"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("AndIf"));
    assert!(stdout.contains("OrIf"));
    assert!(stdout.contains("Pipe"));
}

#[test]
fn cli_lex_redirects() {
    let (stdout, _, code) = run_cli(&["lex", "-c", "cat < in > out >> log"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("RedirectFromFile"));
    assert!(stdout.contains("RedirectToFile"));
    assert!(stdout.contains("Append"));
}

#[test]
fn cli_lex_semicolon_and_amp() {
    let (stdout, _, code) = run_cli(&["lex", "-c", "a; b &"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Semicolon"));
    assert!(stdout.contains("Ampersand"));
}

#[test]
fn cli_lex_newlines() {
    let (stdout, _, code) = run_exec_with_args(&["lex", "-"], "a\nb");
    assert_eq!(code, 0);
    assert!(stdout.contains("Newline"));
    assert!(stdout.contains("\\n"));
}

#[test]
fn cli_lex_io_number() {
    let (stdout, _, code) = run_cli(&["lex", "-c", "cmd 2>&1"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("IoNumber"));
    assert!(stdout.contains("RedirectToFd"));
}

#[test]
fn cli_lex_with_bash_flag() {
    let (stdout, _, code) = run_cli(&["lex", "--bash", "-c", "cmd |& cat"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("BashPipeAmpersand"));
}

#[test]
fn cli_lex_error() {
    let (_, stderr, code) = run_cli(&["lex", "-c", "echo 'unterminated"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("error:"));
}

#[test]
fn cli_lex_from_stdin() {
    let (stdout, _, code) = run_exec_with_args(&["lex", "-"], "true; false");
    assert_eq!(code, 0);
    assert!(stdout.contains("Literal"));
    assert!(stdout.contains("Semicolon"));
    assert!(stdout.contains("true"));
    assert!(stdout.contains("false"));
}
