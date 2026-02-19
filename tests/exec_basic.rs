mod common;

use shell_parser::exec::{ExecError, Executor};
use shell_parser::Dialect;

/// Parse and execute a script, capturing stdout. Returns (stdout, exit_status).
fn exec_ok(script: &str) -> (String, i32) {
    let program = shell_parser::parse(script)
        .unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    let mut executor = Executor::new();
    // Use a controlled PATH for tests
    let _ = executor
        .env_mut()
        .set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");

    match executor.execute(&program) {
        Ok(status) => {
            // We can't easily capture stdout from external commands in-process.
            // For now, return empty stdout and just the status.
            (String::new(), status)
        }
        Err(ExecError::ExitRequested(code)) => (String::new(), code),
        Err(e) => panic!("exec failed for {:?}: {}", script, e),
    }
}

/// Parse and execute a script, returning only the exit status.
fn exec_status(script: &str) -> i32 {
    exec_ok(script).1
}

// --- Basic command execution ---

#[test]
fn true_command() {
    assert_eq!(exec_status("true"), 0);
}

#[test]
fn false_command() {
    assert_eq!(exec_status("false"), 1);
}

#[test]
fn colon_noop() {
    assert_eq!(exec_status(":"), 0);
}

#[test]
fn exit_zero() {
    assert_eq!(exec_status("exit 0"), 0);
}

#[test]
fn exit_nonzero() {
    assert_eq!(exec_status("exit 42"), 42);
}

// --- Variable assignment ---

#[test]
fn variable_assignment_and_echo() {
    // Just test that assignment doesn't error — we can't capture echo output yet.
    assert_eq!(exec_status("X=hello"), 0);
}

#[test]
fn variable_used_in_later_command() {
    // X=hello; exit status of assignment is 0
    let program = shell_parser::parse("X=hello\ntrue").unwrap();
    let mut executor = Executor::new();
    let status = executor.execute(&program).unwrap();
    assert_eq!(status, 0);
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

// --- AND/OR lists ---

#[test]
fn and_list_both_true() {
    assert_eq!(exec_status("true && true"), 0);
}

#[test]
fn and_list_first_false() {
    assert_eq!(exec_status("false && true"), 1);
}

#[test]
fn or_list_first_false() {
    assert_eq!(exec_status("false || true"), 0);
}

#[test]
fn or_list_first_true() {
    assert_eq!(exec_status("true || false"), 0);
}

// --- Not ---

#[test]
fn not_true() {
    assert_eq!(exec_status("! true"), 1);
}

#[test]
fn not_false() {
    assert_eq!(exec_status("! false"), 0);
}

// --- Multiple statements ---

#[test]
fn multiple_statements_last_status() {
    assert_eq!(exec_status("true; false"), 1);
    assert_eq!(exec_status("false; true"), 0);
}

// --- exit status propagation ---

#[test]
fn exit_status_variable() {
    let program = shell_parser::parse("false\ntrue").unwrap();
    let mut executor = Executor::new();

    // After executing, last exit status should be from `true` (0).
    let status = executor.execute(&program).unwrap();
    assert_eq!(status, 0);
}

// --- If statements ---

#[test]
fn if_true_branch() {
    let program = shell_parser::parse("if true; then X=yes; else X=no; fi").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("yes"));
}

#[test]
fn if_false_branch() {
    let program = shell_parser::parse("if false; then X=yes; else X=no; fi").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("no"));
}

#[test]
fn if_no_else_false() {
    let program = shell_parser::parse("if false; then X=yes; fi").unwrap();
    let mut executor = Executor::new();
    let status = executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), None);
    assert_eq!(status, 0);
}

// --- While loop ---

#[test]
fn while_loop_counts() {
    // Arithmetic expansion not yet implemented, so use a simpler test.
    // This test currently tests the while structure only.
    let program = shell_parser::parse("X=0\nwhile test $X != done; do X=done; done").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("done"));
}

// --- For loop ---

#[test]
fn for_loop_over_words() {
    let program = shell_parser::parse("RESULT=\nfor i in a b c; do RESULT=${RESULT}${i}; done").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("RESULT"), Some("abc"));
}

// --- Case statement ---

#[test]
fn case_exact_match() {
    let program = shell_parser::parse(r#"
case hello in
    hello) X=matched ;;
    *) X=default ;;
esac
"#).unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("matched"));
}

#[test]
fn case_wildcard_match() {
    let program = shell_parser::parse(r#"
case world in
    hello) X=hello ;;
    *) X=default ;;
esac
"#).unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("default"));
}

// --- Brace group ---

#[test]
fn brace_group() {
    let program = shell_parser::parse("{ X=inside; }").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("inside"));
}

// --- Function definition and call ---

#[test]
fn function_define_and_call() {
    let program = shell_parser::parse("greet() { X=hello; }\ngreet").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

// --- Export ---

#[test]
fn export_builtin() {
    let program = shell_parser::parse("export FOO=bar").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("FOO"), Some("bar"));
    assert!(executor.env().is_exported("FOO"));
}

// --- Unset ---

#[test]
fn unset_builtin() {
    let program = shell_parser::parse("X=hello\nunset X").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), None);
}

// --- External command (basic smoke test) ---

#[test]
fn external_command_true() {
    // /usr/bin/true should exist on any Unix system
    assert_eq!(exec_status("/usr/bin/true"), 0);
}

#[test]
fn external_command_false() {
    assert_eq!(exec_status("/usr/bin/false"), 1);
}

#[test]
fn external_command_not_found() {
    assert_eq!(exec_status("nonexistent_command_xyz_123"), 127);
}

// --- Test builtin ---

#[test]
fn test_builtin_string() {
    assert_eq!(exec_status("test hello"), 0);
    assert_eq!(exec_status("test ''"), 1);
}

#[test]
fn test_builtin_eq() {
    assert_eq!(exec_status("test 5 -eq 5"), 0);
    assert_eq!(exec_status("test 5 -eq 6"), 1);
}

#[test]
fn bracket_test_syntax() {
    assert_eq!(exec_status("[ hello ]"), 0);
    assert_eq!(exec_status("[ 3 -gt 2 ]"), 0);
}

// --- Break/continue ---

#[test]
fn break_in_while() {
    let program = shell_parser::parse(r#"
X=0
while true; do
    X=1
    break
    X=2
done
"#).unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("1"));
}

#[test]
fn continue_in_for() {
    let program = shell_parser::parse(r#"
RESULT=
for i in a skip b; do
    if test "$i" = skip; then
        continue
    fi
    RESULT=${RESULT}${i}
done
"#).unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("RESULT"), Some("ab"));
}

// --- Command substitution ---

#[test]
fn command_substitution_builtin() {
    let program = shell_parser::parse("X=$(echo hello)").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

#[test]
fn command_substitution_external() {
    let program = shell_parser::parse("X=$(/bin/echo world)").unwrap();
    let mut executor = Executor::new();
    let _ = executor.env_mut().set_var("PATH", "/usr/bin:/bin");
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("world"));
}

#[test]
fn command_substitution_strips_trailing_newlines() {
    let program = shell_parser::parse("X=$(echo hello)").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    // echo produces "hello\n", cmd sub strips trailing newlines
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

#[test]
fn command_substitution_in_argument() {
    // Test that $(...) works in command arguments
    let program = shell_parser::parse("X=$(echo inner)\nY=${X}").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("inner"));
    assert_eq!(executor.env().get_var("Y"), Some("inner"));
}

#[test]
fn command_substitution_exit_status() {
    let program = shell_parser::parse("X=$(false)").unwrap();
    let mut executor = Executor::new();
    executor.execute(&program).unwrap();
    // $? should reflect the command substitution's exit status
    // (though the assignment itself succeeds with status 0)
    assert_eq!(executor.env().get_var("X"), Some(""));
}

// --- Unsupported features produce explicit errors ---

fn expect_unsupported(script: &str) {
    let program = shell_parser::parse(script)
        .unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));
    let mut executor = Executor::new();
    let _ = executor
        .env_mut()
        .set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let err = executor
        .execute(&program)
        .expect_err(&format!("expected UnsupportedFeature for {:?}", script));
    assert!(
        matches!(err, ExecError::UnsupportedFeature(_)),
        "expected UnsupportedFeature, got {:?} for {:?}",
        err,
        script,
    );
}

fn expect_unsupported_bash(script: &str) {
    let program = shell_parser::parse_with(script, Dialect::Bash)
        .unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));
    let mut executor = Executor::new();
    let err = executor
        .execute(&program)
        .expect_err(&format!("expected UnsupportedFeature for {:?}", script));
    assert!(
        matches!(err, ExecError::UnsupportedFeature(_)),
        "expected UnsupportedFeature, got {:?} for {:?}",
        err,
        script,
    );
}

#[test]
fn unsupported_background() {
    expect_unsupported("echo hello &");
}

#[test]
fn unsupported_arithmetic_expansion() {
    expect_unsupported("echo $((1+2))");
}

#[test]
fn unsupported_heredoc() {
    expect_unsupported("cat <<EOF\nhello\nEOF");
}

#[test]
fn unsupported_compound_redirect() {
    expect_unsupported("if true; then echo hi; fi > /tmp/claude/test-out");
}

#[test]
fn unsupported_subshell() {
    expect_unsupported("(echo hello)");
}

#[test]
fn unsupported_set_options() {
    expect_unsupported("set -e");
}

#[test]
fn unsupported_pattern_trim() {
    expect_unsupported("X=hello.txt; echo ${X%.txt}");
}

#[test]
fn unsupported_bash_double_bracket() {
    expect_unsupported_bash("[[ -n hello ]]");
}

#[test]
fn unsupported_eval_builtin() {
    expect_unsupported("eval echo hello");
}

#[test]
fn unsupported_default_assign() {
    expect_unsupported("echo ${UNSET_VAR:=default}");
}
