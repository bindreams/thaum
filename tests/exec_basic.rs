mod common;

use thaum::exec::{CapturedIo, ExecError, Executor};
use thaum::Dialect;

/// Parse and execute a script, capturing stdout. Returns (stdout, exit_status).
fn exec_ok(script: &str) -> (String, i32) {
    let program = thaum::parse(script)
        .unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    let mut executor = Executor::new();
    // Use a controlled PATH for tests
    let _ = executor
        .env_mut()
        .set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");

    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => (captured.stdout_string(), status),
        Err(ExecError::ExitRequested(code)) => (captured.stdout_string(), code),
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
    let program = thaum::parse("X=hello\ntrue").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let status = executor.execute(&program, &mut captured.context()).unwrap();
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
    let program = thaum::parse("false\ntrue").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();

    // After executing, last exit status should be from `true` (0).
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(status, 0);
}

// --- If statements ---

#[test]
fn if_true_branch() {
    let program = thaum::parse("if true; then X=yes; else X=no; fi").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("yes"));
}

#[test]
fn if_false_branch() {
    let program = thaum::parse("if false; then X=yes; else X=no; fi").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("no"));
}

#[test]
fn if_no_else_false() {
    let program = thaum::parse("if false; then X=yes; fi").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), None);
    assert_eq!(status, 0);
}

// --- While loop ---

#[test]
fn while_loop_counts() {
    // Arithmetic expansion not yet implemented, so use a simpler test.
    // This test currently tests the while structure only.
    let program = thaum::parse("X=0\nwhile test $X != done; do X=done; done").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("done"));
}

// --- For loop ---

#[test]
fn for_loop_over_words() {
    let program = thaum::parse("RESULT=\nfor i in a b c; do RESULT=${RESULT}${i}; done").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("RESULT"), Some("abc"));
}

// --- Case statement ---

#[test]
fn case_exact_match() {
    let program = thaum::parse(r#"
case hello in
    hello) X=matched ;;
    *) X=default ;;
esac
"#).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("matched"));
}

#[test]
fn case_wildcard_match() {
    let program = thaum::parse(r#"
case world in
    hello) X=hello ;;
    *) X=default ;;
esac
"#).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("default"));
}

// --- Brace group ---

#[test]
fn brace_group() {
    let program = thaum::parse("{ X=inside; }").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("inside"));
}

// --- Function definition and call ---

#[test]
fn function_define_and_call() {
    let program = thaum::parse("greet() { X=hello; }\ngreet").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

// --- Export ---

#[test]
fn export_builtin() {
    let program = thaum::parse("export FOO=bar").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("FOO"), Some("bar"));
    assert!(executor.env().is_exported("FOO"));
}

// --- Unset ---

#[test]
fn unset_builtin() {
    let program = thaum::parse("X=hello\nunset X").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
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
    let program = thaum::parse(r#"
X=0
while true; do
    X=1
    break
    X=2
done
"#).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("1"));
}

#[test]
fn continue_in_for() {
    let program = thaum::parse(r#"
RESULT=
for i in a skip b; do
    if test "$i" = skip; then
        continue
    fi
    RESULT=${RESULT}${i}
done
"#).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("RESULT"), Some("ab"));
}

// --- Command substitution ---

#[test]
fn command_substitution_builtin() {
    let program = thaum::parse("X=$(echo hello)").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

#[test]
fn command_substitution_external() {
    let program = thaum::parse("X=$(/bin/echo world)").unwrap();
    let mut executor = Executor::new();
    let _ = executor.env_mut().set_var("PATH", "/usr/bin:/bin");
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("world"));
}

#[test]
fn command_substitution_strips_trailing_newlines() {
    let program = thaum::parse("X=$(echo hello)").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    // echo produces "hello\n", cmd sub strips trailing newlines
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

#[test]
fn command_substitution_in_argument() {
    // Test that $(...) works in command arguments
    let program = thaum::parse("X=$(echo inner)\nY=${X}").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("inner"));
    assert_eq!(executor.env().get_var("Y"), Some("inner"));
}

#[test]
fn command_substitution_exit_status() {
    let program = thaum::parse("X=$(false)").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    // $? should reflect the command substitution's exit status
    // (though the assignment itself succeeds with status 0)
    assert_eq!(executor.env().get_var("X"), Some(""));
}

// --- Unsupported features produce explicit errors ---

fn expect_unsupported(script: &str) {
    let program = thaum::parse(script)
        .unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));
    let mut executor = Executor::new();
    let _ = executor
        .env_mut()
        .set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let mut captured = CapturedIo::new();
    let err = executor
        .execute(&program, &mut captured.context())
        .expect_err(&format!("expected UnsupportedFeature for {:?}", script));
    assert!(
        matches!(err, ExecError::UnsupportedFeature(_)),
        "expected UnsupportedFeature, got {:?} for {:?}",
        err,
        script,
    );
}

fn expect_unsupported_bash(script: &str) {
    let program = thaum::parse_with(script, Dialect::Bash)
        .unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let err = executor
        .execute(&program, &mut captured.context())
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

// --- Arithmetic expansion $((expr)) ---

#[test]
fn arith_expansion_simple() {
    let program = thaum::parse("X=$((1+2))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("3"));
}

#[test]
fn arith_expansion_with_variables() {
    let program = thaum::parse("A=10\nX=$((A+5))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("15"));
}

#[test]
fn arith_expansion_in_double_quotes() {
    let program = thaum::parse(r#"X="val: $((2*3))""#).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("val: 6"));
}

#[test]
fn arith_expansion_with_assignment_side_effect() {
    let program = thaum::parse("X=$((y=5))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("5"));
    assert_eq!(executor.env().get_var("y"), Some("5"));
}

#[test]
fn arith_expansion_division_by_zero() {
    let program = thaum::parse("X=$((1/0))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let err = executor.execute(&program, &mut captured.context()).unwrap_err();
    assert!(matches!(err, ExecError::DivisionByZero));
}

#[test]
fn arith_expansion_nested_ops() {
    let program = thaum::parse("X=$(( (2 + 3) * 4 ))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("20"));
}

#[test]
fn arith_expansion_unset_var_is_zero() {
    let program = thaum::parse("X=$((UNSET + 1))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("1"));
}

// --- Bash (( )) arithmetic command ---

#[test]
fn bash_arith_command_nonzero_is_success() {
    let program = thaum::parse_with("(( 5 ))", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    assert_eq!(executor.execute(&program, &mut captured.context()).unwrap(), 0);
}

#[test]
fn bash_arith_command_zero_is_failure() {
    let program = thaum::parse_with("(( 0 ))", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    assert_eq!(executor.execute(&program, &mut captured.context()).unwrap(), 1);
}

#[test]
fn bash_arith_command_with_assignment() {
    let program = thaum::parse_with("(( x = 42 ))", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(status, 0); // 42 != 0 → success
    assert_eq!(executor.env().get_var("x"), Some("42"));
}

// --- Bash for (( )) arithmetic for loop ---

#[test]
fn bash_arith_for_basic() {
    let program = thaum::parse_with(
        "for ((i=0; i<5; i++)); do true; done",
        Dialect::Bash,
    ).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("i"), Some("5"));
}

#[test]
fn bash_arith_for_sum() {
    let program = thaum::parse_with(
        "sum=0\nfor ((i=1; i<=10; i++)); do sum=$((sum+i)); done",
        Dialect::Bash,
    ).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("sum"), Some("55"));
}

#[test]
fn bash_arith_for_break() {
    let program = thaum::parse_with(
        "for ((i=0; i<100; i++)); do if test $i -eq 3; then break; fi; done",
        Dialect::Bash,
    ).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("i"), Some("3"));
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

// --- Pattern trimming ---

#[test]
fn trim_small_suffix() {
    let (out, _) = exec_ok("X=hello.txt; echo ${X%.txt}");
    assert_eq!(out, "hello\n");
}

#[test]
fn trim_large_suffix() {
    let (out, _) = exec_ok("X=archive.tar.gz; echo ${X%%.*}");
    assert_eq!(out, "archive\n");
}

#[test]
fn trim_small_prefix() {
    let (out, _) = exec_ok("X=/usr/bin:/usr/local/bin; echo ${X#*/}");
    assert_eq!(out, "usr/bin:/usr/local/bin\n");
}

#[test]
fn trim_large_prefix() {
    // ${X##*/} extracts basename
    let (out, _) = exec_ok("X=/a/b/c.txt; echo ${X##*/}");
    assert_eq!(out, "c.txt\n");
}

#[test]
fn unsupported_bash_double_bracket() {
    expect_unsupported_bash("[[ -n hello ]]");
}

#[test]
fn unsupported_eval_builtin() {
    expect_unsupported("eval echo hello");
}

// --- DefaultAssign (${var:=default}) ---

#[test]
fn default_assign_when_unset() {
    let (out, status) = exec_ok("echo ${X:=hello}; echo $X");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\nhello\n");
}

#[test]
fn default_assign_when_set() {
    let (out, status) = exec_ok("X=existing; echo ${X:=fallback}; echo $X");
    assert_eq!(status, 0);
    assert_eq!(out, "existing\nexisting\n");
}
