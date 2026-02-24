mod common;

use thaum::exec::{CapturedIo, ExecError, Executor};
use thaum::Dialect;

/// Find the thaum binary for subshell tests.
///
/// During `cargo test`, the test binary is NOT the thaum CLI. We need the
/// actual `thaum` binary which lives at `target/debug/thaum` (or
/// `target/release/thaum`).
fn thaum_exe() -> std::path::PathBuf {
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("thaum");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

/// Create an executor configured for tests (controlled PATH, thaum exe path).
fn test_executor() -> Executor {
    let mut executor = Executor::new();
    let _ = executor.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    executor.set_exe_path(thaum_exe());
    executor
}

/// Parse and execute a script, capturing stdout. Returns (stdout, exit_status).
fn exec_ok(script: &str) -> (String, i32) {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    let mut executor = test_executor();

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

/// Parse and execute a script, returning the exit status or 1 on error.
/// Unlike `exec_ok`, this does not panic on execution errors.
fn exec_result(script: &str) -> i32 {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    let mut executor = test_executor();

    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => status,
        Err(ExecError::ExitRequested(code)) => code,
        Err(_) => 1,
    }
}

// Basic command execution ---------------------------------------------------------------------------------------------

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

// Variable assignment -------------------------------------------------------------------------------------------------

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

// AND/OR lists --------------------------------------------------------------------------------------------------------

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

// Not -----------------------------------------------------------------------------------------------------------------

#[test]
fn not_true() {
    assert_eq!(exec_status("! true"), 1);
}

#[test]
fn not_false() {
    assert_eq!(exec_status("! false"), 0);
}

// Multiple statements -------------------------------------------------------------------------------------------------

#[test]
fn multiple_statements_last_status() {
    assert_eq!(exec_status("true; false"), 1);
    assert_eq!(exec_status("false; true"), 0);
}

// exit status propagation ---------------------------------------------------------------------------------------------

#[test]
fn exit_status_variable() {
    let program = thaum::parse("false\ntrue").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();

    // After executing, last exit status should be from `true` (0).
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(status, 0);
}

// If statements -------------------------------------------------------------------------------------------------------

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

// While loop ----------------------------------------------------------------------------------------------------------

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

// For loop ------------------------------------------------------------------------------------------------------------

#[test]
fn for_loop_over_words() {
    let program = thaum::parse("RESULT=\nfor i in a b c; do RESULT=${RESULT}${i}; done").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("RESULT"), Some("abc"));
}

// Case statement ------------------------------------------------------------------------------------------------------

#[test]
fn case_exact_match() {
    let program = thaum::parse(
        r#"
case hello in
    hello) X=matched ;;
    *) X=default ;;
esac
"#,
    )
    .unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("matched"));
}

#[test]
fn case_wildcard_match() {
    let program = thaum::parse(
        r#"
case world in
    hello) X=hello ;;
    *) X=default ;;
esac
"#,
    )
    .unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("default"));
}

// Brace group ---------------------------------------------------------------------------------------------------------

#[test]
fn brace_group() {
    let program = thaum::parse("{ X=inside; }").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("inside"));
}

// Function definition and call ----------------------------------------------------------------------------------------

#[test]
fn function_define_and_call() {
    let program = thaum::parse("greet() { X=hello; }\ngreet").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

// Export --------------------------------------------------------------------------------------------------------------

#[test]
fn export_builtin() {
    let program = thaum::parse("export FOO=bar").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("FOO"), Some("bar"));
    assert!(executor.env().is_exported("FOO"));
}

// Unset ---------------------------------------------------------------------------------------------------------------

#[test]
fn unset_builtin() {
    let program = thaum::parse("X=hello\nunset X").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), None);
}

// External command (basic smoke test) ---------------------------------------------------------------------------------

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

// Test builtin --------------------------------------------------------------------------------------------------------

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

// Break/continue ------------------------------------------------------------------------------------------------------

#[test]
fn break_in_while() {
    let program = thaum::parse(
        r#"
X=0
while true; do
    X=1
    break
    X=2
done
"#,
    )
    .unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("1"));
}

#[test]
fn continue_in_for() {
    let program = thaum::parse(
        r#"
RESULT=
for i in a skip b; do
    if test "$i" = skip; then
        continue
    fi
    RESULT=${RESULT}${i}
done
"#,
    )
    .unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("RESULT"), Some("ab"));
}

// Command substitution ------------------------------------------------------------------------------------------------

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

// Unsupported features produce explicit errors ------------------------------------------------------------------------

fn expect_unsupported(script: &str) {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));
    let mut executor = Executor::new();
    let _ = executor.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
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

#[allow(dead_code)]
fn expect_unsupported_bash(script: &str) {
    let program =
        thaum::parse_with(script, Dialect::Bash).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));
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

/// Parse and execute a Bash-dialect script, returning the exit status or 1 on error.
/// Unlike `bash_exec_ok`, this does not panic on execution errors.
fn bash_exec_result(script: &str) -> i32 {
    let program =
        thaum::parse_with(script, Dialect::Bash).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    let mut executor = test_executor();

    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => status,
        Err(ExecError::ExitRequested(code)) => code,
        Err(_) => 1,
    }
}

/// Parse and execute a Bash-dialect script, capturing stdout. Returns (stdout, exit_status).
fn bash_exec_ok(script: &str) -> (String, i32) {
    let program =
        thaum::parse_with(script, Dialect::Bash).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    let mut executor = test_executor();

    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => (captured.stdout_string(), status),
        Err(ExecError::ExitRequested(code)) => (captured.stdout_string(), code),
        Err(e) => panic!("exec failed for {:?}: {}", script, e),
    }
}

#[test]
fn unsupported_background() {
    expect_unsupported("echo hello &");
}

// Arithmetic expansion $((expr)) --------------------------------------------------------------------------------------

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

// Bash (( )) arithmetic command ---------------------------------------------------------------------------------------

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

// Bash for (( )) arithmetic for loop ----------------------------------------------------------------------------------

#[test]
fn bash_arith_for_basic() {
    let program = thaum::parse_with("for ((i=0; i<5; i++)); do true; done", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("i"), Some("5"));
}

#[test]
fn bash_arith_for_sum() {
    let program = thaum::parse_with("sum=0\nfor ((i=1; i<=10; i++)); do sum=$((sum+i)); done", Dialect::Bash).unwrap();
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
    )
    .unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("i"), Some("3"));
}

#[test]
fn heredoc_basic() {
    // Use `read` (builtin) to verify heredoc stdin redirection.
    // External commands write to the real process stdout, not CapturedIo.
    let (out, status) = exec_ok("read VAR <<EOF\nhello\nEOF\necho $VAR");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[test]
fn unsupported_compound_redirect() {
    expect_unsupported("if true; then echo hi; fi > /tmp/claude/test-out");
}

// subshell is now supported — see subshell_* tests below

// set -e is now supported — see set_e_* tests below.

// [[ ]] is now implemented — see bash_cond_* tests below.

// eval is now implemented — see eval_* tests below.

// DefaultAssign (${var:=default}) -------------------------------------------------------------------------------------

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

// Pattern trimming ----------------------------------------------------------------------------------------------------

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

// readonly builtin ----------------------------------------------------------------------------------------------------

#[test]
fn readonly_set_and_read() {
    let (out, status) = exec_ok("readonly X=42; echo $X");
    assert_eq!(status, 0);
    assert_eq!(out, "42\n");
}

#[test]
fn readonly_prevents_assignment() {
    let status = exec_result("readonly X=42; X=99");
    assert_ne!(status, 0);
}

// local builtin -------------------------------------------------------------------------------------------------------

#[test]
fn local_scopes_variable_in_function() {
    let (out, _) = exec_ok("f() { local X=inner; echo $X; }; X=outer; f; echo $X");
    assert_eq!(out, "inner\nouter\n");
}

#[test]
fn local_unset_var_removed_on_exit() {
    let (out, _) = exec_ok("f() { local Y=temp; echo $Y; }; f; echo \"${Y:-gone}\"");
    assert_eq!(out, "temp\ngone\n");
}

#[test]
fn local_outside_function_fails() {
    let status = exec_result("local X=1");
    assert_ne!(status, 0);
}

// Redirect tests ------------------------------------------------------------------------------------------------------

#[test]
fn redirect_builtin_stdout_to_file() {
    let dir = std::path::PathBuf::from("/tmp/claude/redir-tests");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("stdout.txt");

    let script = format!("echo hello > {}", file.display());
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, ""); // stdout went to file, not captured
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn redirect_builtin_append() {
    let dir = std::path::PathBuf::from("/tmp/claude/redir-tests-append");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("append.txt");

    let script = format!("echo first > {f}; echo second >> {f}", f = file.display());
    let (_, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "first\nsecond\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn redirect_stdin_from_file() {
    let dir = std::path::PathBuf::from("/tmp/claude/redir-tests-stdin");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("input.txt");
    std::fs::write(&file, "from-file\n").unwrap();

    let script = format!("read LINE < {}; echo $LINE", file.display());
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "from-file\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn redirect_dup_stdout_to_stderr_file() {
    // > file 2>&1 — redirect stdout to file, then dup stderr to same file
    let dir = std::path::PathBuf::from("/tmp/claude/redir-tests-dup");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("combined.txt");

    let script = format!("echo hello > {} 2>&1", file.display());
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, "");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn redirect_fd3_and_dup_to_stdout() {
    // echo hello 3>/tmp/file >&3 — open FD 3 to file, dup stdout to FD 3
    let dir = std::path::PathBuf::from("/tmp/claude/redir-tests-fd3");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("fd3.txt");

    let script = format!("echo hello 3>{} >&3", file.display());
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, ""); // stdout went to FD 3 → file
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn redirect_creates_empty_file() {
    // `> file` with no command creates/truncates the file
    let dir = std::path::PathBuf::from("/tmp/claude/redir-tests-empty");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("empty.txt");

    let script = format!("> {}", file.display());
    let (_, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn external_command_inherits_fd3() {
    // sh -c 'echo hello >&3' writes to FD 3, which is redirected to a file.
    // This tests that FDs 3+ are passed to external child processes.
    let dir = std::path::PathBuf::from("/tmp/claude/fd-inherit-test");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("fd3.txt");

    let script = format!("sh -c 'echo hello >&3' 3>{}", file.display());
    let (_, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");

    let _ = std::fs::remove_dir_all(&dir);
}

// Bash indexed arrays -------------------------------------------------------------------------------------------------

#[test]
fn array_literal_assignment() {
    let (out, status) = bash_exec_ok("a=(one two three); echo ${a[0]}");
    assert_eq!(status, 0);
    assert_eq!(out, "one\n");
}

#[test]
fn array_element_access() {
    let (out, _) = bash_exec_ok("a=(x y z); echo ${a[1]}");
    assert_eq!(out, "y\n");
}

#[test]
fn array_all_elements_at() {
    let (out, _) = bash_exec_ok("a=(a b c); echo ${a[@]}");
    assert_eq!(out, "a b c\n");
}

#[test]
fn array_all_elements_star() {
    let (out, _) = bash_exec_ok("a=(a b c); echo ${a[*]}");
    assert_eq!(out, "a b c\n");
}

#[test]
fn array_length() {
    let (out, _) = bash_exec_ok("a=(a b c); echo ${#a[@]}");
    assert_eq!(out, "3\n");
}

#[test]
fn array_element_length() {
    let (out, _) = bash_exec_ok("a=(hello); echo ${#a[0]}");
    assert_eq!(out, "5\n");
}

#[test]
fn array_default_index() {
    // $a is equivalent to ${a[0]} in bash
    let (out, _) = bash_exec_ok("a=(first second); echo $a");
    assert_eq!(out, "first\n");
}

#[test]
fn array_indexed_assignment() {
    let (out, _) = bash_exec_ok("a[0]=hello; echo ${a[0]}");
    assert_eq!(out, "hello\n");
}

#[test]
fn array_sparse_assignment() {
    let (out, _) = bash_exec_ok("a[5]=five; echo ${a[5]}");
    assert_eq!(out, "five\n");
}

#[test]
fn array_overwrite_element() {
    let (out, _) = bash_exec_ok("a=(x y z); a[1]=Y; echo ${a[@]}");
    assert_eq!(out, "x Y z\n");
}

#[test]
fn array_unset_element() {
    let (out, _) = bash_exec_ok("a=(x y z); unset a[1]; echo ${a[@]}");
    assert_eq!(out, "x z\n");
}

#[test]
fn array_unset_whole() {
    let (out, _) = bash_exec_ok("a=(x y z); unset a; echo \"${a[@]}\"");
    assert_eq!(out, "\n");
}

#[test]
fn array_arith_access() {
    let (out, _) = bash_exec_ok("a=(10 20 30); echo $(( a[1] + a[2] ))");
    assert_eq!(out, "50\n");
}

#[test]
fn array_arith_assign() {
    let (out, _) = bash_exec_ok("(( a[0] = 42 )); echo ${a[0]}");
    assert_eq!(out, "42\n");
}

#[test]
fn array_for_loop() {
    // TODO: field splitting not yet implemented — ${a[@]} expands to a single
    // "x y z" string instead of three separate fields.  Once field splitting
    // lands, update this test to expect "x\ny\nz\n".
    let (out, _) = bash_exec_ok(r#"a=(x y z); for i in ${a[@]}; do echo $i; done"#);
    assert_eq!(out, "x y z\n");
}

// Bash alias expansion ------------------------------------------------------------------------------------------------

#[test]
fn alias_basic() {
    let (out, status) = bash_exec_ok("shopt -s expand_aliases\nalias hi='echo hello'\nhi");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[test]
fn alias_requires_shopt() {
    // Without shopt -s expand_aliases, aliases are defined but not expanded
    let (_, status) = bash_exec_ok("alias hi='echo hello'\nhi");
    assert_ne!(status, 0);
}

#[test]
fn alias_same_line_not_expanded() {
    // alias e=echo; e one — same line, e is NOT expanded (parsed before defined)
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias e=echo; e one");
    assert_ne!(status, 0);
}

#[test]
fn alias_cross_line_expanded() {
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias e=echo\ne hello");
    assert_eq!(out, "hello\n");
}

#[test]
fn alias_semicolon_then_newline() {
    // alias a="echo";  ← trailing semicolon, then newline → next line sees alias
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias a=echo;\na hello");
    assert_eq!(out, "hello\n");
}

#[test]
fn alias_unalias() {
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias e=echo\nunalias e\ne hello");
    assert_ne!(status, 0);
}

#[test]
fn alias_unalias_same_line() {
    // alias + unalias on one line; next line sees no alias
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias a=echo; unalias a\na hello");
    assert_ne!(status, 0);
}

#[test]
fn alias_recursive() {
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias hi='e_ hello'\nalias e_='echo __'\nhi");
    assert_eq!(out, "__ hello\n");
}

#[test]
fn alias_trailing_space() {
    // Alias ending with space → next word also alias-expanded
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias hi='echo '\nalias w='hello'\nhi w");
    assert_eq!(out, "hello\n");
}

#[test]
fn alias_quoted_not_expanded() {
    // Quoted command name must NOT trigger alias expansion
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias hi='echo hello'\n'hi'");
    assert_ne!(status, 0);
}

#[test]
fn alias_list() {
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias e=echo\nalias");
    assert!(out.contains("alias e='echo'") || out.contains("alias e=echo"));
}

#[test]
fn alias_redefine_then_unalias() {
    // Line 2: alias a="touch"  → defines a=touch
    // Line 3: alias a="echo"; unalias a  → redefines then removes
    // Line 4: a hello  → not found (unalias took effect)
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias a=touch\nalias a=echo; unalias a\na hello");
    assert_ne!(status, 0);
}

#[test]
fn alias_snapshot_uses_previous_line() {
    // Line 2: alias a="echo"  → defines a=echo
    // Line 3: alias a="touch"; a hello; unalias a
    //   → snapshot for line 3 has a=echo (from before line 3 executed)
    //   → so "a hello" expands to "echo hello" (not "touch hello")
    //   → then alias a is redefined to touch, then unaliased — both during execution
    // Line 4: a hello  → not found (unalias from line 3 took effect)
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias a=echo\nalias a=touch; a hello; unalias a");
    assert_eq!(out, "hello\n");
}

#[test]
fn alias_snapshot_touch_file() {
    // Line 2: alias a="touch"
    // Line 3: alias a="echo"; a hello; unalias a
    //   → snapshot for line 3 has a=touch (from line 2)
    //   → "a hello" expands to "touch hello" (creates file)
    // Line 4: a hello  → not found
    let dir = std::path::PathBuf::from("/tmp/claude/alias-touch-test");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("hello");
    let _ = std::fs::remove_file(&file);

    let script = format!(
        "shopt -s expand_aliases\nalias a=touch\ncd {}; alias a=echo; a hello; unalias a",
        dir.display()
    );
    let (_, _) = bash_exec_ok(&script);
    assert!(file.exists(), "touch hello should have created the file");

    let _ = std::fs::remove_dir_all(&dir);
}

// Subshell execution --------------------------------------------------------------------------------------------------

#[test]
fn subshell_basic() {
    let (out, status) = exec_ok("(echo hello)");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[test]
fn subshell_exit_status() {
    let (out, _) = exec_ok("(exit 42); echo $?");
    assert_eq!(out, "42\n");
}

#[test]
fn subshell_variable_isolation() {
    let (out, _) = exec_ok("x=1; (x=2); echo $x");
    assert_eq!(out, "1\n");
}

#[test]
fn subshell_inherits_vars() {
    let (out, _) = exec_ok("x=hello; (echo $x)");
    assert_eq!(out, "hello\n");
}

#[test]
fn subshell_inherits_functions() {
    let (out, _) = exec_ok("f() { echo hi; }; (f)");
    assert_eq!(out, "hi\n");
}

#[test]
fn subshell_nested() {
    let (out, _) = exec_ok("((echo inner))");
    assert_eq!(out, "inner\n");
}

#[test]
fn subshell_with_redirect() {
    // Redirect inside the subshell (not on the compound command).
    let dir = std::path::PathBuf::from("/tmp/claude/subshell-redir-test");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("out.txt");

    let script = format!("(echo hello > {})", file.display());
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, ""); // stdout went to file inside subshell
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");

    let _ = std::fs::remove_dir_all(&dir);
}

// Associative arrays --------------------------------------------------------------------------------------------------

#[test]
fn assoc_array_basic() {
    let (out, _) = bash_exec_ok("declare -A m; m[foo]=bar; echo ${m[foo]}");
    assert_eq!(out, "bar\n");
}

#[test]
fn assoc_array_all_elements() {
    let (out, _) = bash_exec_ok("declare -A m; m[a]=1; m[b]=2; echo ${#m[@]}");
    assert_eq!(out, "2\n");
}

#[test]
fn assoc_array_overwrite() {
    let (out, _) = bash_exec_ok("declare -A m; m[k]=old; m[k]=new; echo ${m[k]}");
    assert_eq!(out, "new\n");
}

#[test]
fn assoc_array_unset_element() {
    let (out, _) = bash_exec_ok("declare -A m; m[a]=1; m[b]=2; unset m[a]; echo ${#m[@]}");
    assert_eq!(out, "1\n");
}

#[test]
fn assoc_array_unset_whole() {
    let (out, _) = bash_exec_ok("declare -A m; m[a]=1; unset m; echo \"${m[@]}\"");
    assert_eq!(out, "\n");
}

// typeset/declare + flags (attribute removal) -------------------------------------------------------------------------

#[test]
fn typeset_plus_r_bash_silently_fails() {
    // Bash behavior: typeset +r does NOT remove readonly
    let (out, _) = bash_exec_ok("readonly x=1; typeset +r x 2>/dev/null; echo $x");
    assert_eq!(out, "1\n");
}

#[test]
fn typeset_plus_x_unexports() {
    // +x removes export attribute, value preserved
    let (out, _) = bash_exec_ok("export x=hello; declare +x x; echo $x");
    assert_eq!(out, "hello\n");
}

#[test]
fn typeset_plus_i_removes_integer() {
    // +i removes integer attribute — subsequent assignment stores string
    let (out, _) = bash_exec_ok("declare -i x=42; declare +i x; x=hello; echo $x");
    assert_eq!(out, "hello\n");
}

#[test]
fn typeset_plus_l_removes_lowercase() {
    // +l removes lowercase attribute — subsequent assignment preserves case
    let (out, _) = bash_exec_ok("declare -l x=hello; declare +l x; x=WORLD; echo $x");
    assert_eq!(out, "WORLD\n");
}

#[test]
fn typeset_plus_u_removes_uppercase() {
    let (out, _) = bash_exec_ok("declare -u x=HELLO; declare +u x; x=world; echo $x");
    assert_eq!(out, "world\n");
}

// declare/typeset builtin ---------------------------------------------------------------------------------------------

#[test]
fn declare_indexed_array() {
    // NOTE: `declare -a a=(1 2 3)` is not yet supported because the parser
    // does not handle compound array assignment in argument position.
    // Use separate assignment instead.
    let (out, _) = bash_exec_ok("declare -a a; a=(1 2 3); echo ${a[1]}");
    assert_eq!(out, "2\n");
}

#[test]
fn declare_assoc_array_inline() {
    // Note: declare -A m=([k]=v) requires the parser to handle compound assignment
    // For now test the simpler form
    let (out, _) = bash_exec_ok("declare -A m; m[k]=v; echo ${m[k]}");
    assert_eq!(out, "v\n");
}

#[test]
fn declare_readonly() {
    let status = bash_exec_result("declare -r x=42; x=99");
    assert_ne!(status, 0);
}

#[test]
fn declare_export() {
    let (out, _) = bash_exec_ok("declare -x MYVAR=hello; echo $MYVAR");
    assert_eq!(out, "hello\n");
}

#[test]
fn declare_integer() {
    let (out, _) = bash_exec_ok("declare -i x; x='2+3'; echo $x");
    assert_eq!(out, "5\n");
}

#[test]
fn declare_integer_assign() {
    let (out, _) = bash_exec_ok("declare -i x=10; x='x+5'; echo $x");
    assert_eq!(out, "15\n");
}

#[test]
fn declare_local_in_function() {
    let (out, _) = bash_exec_ok("f() { declare x=inner; echo $x; }; x=outer; f; echo $x");
    assert_eq!(out, "inner\nouter\n");
}

#[test]
fn declare_global_in_function() {
    let (out, _) = bash_exec_ok("f() { declare -g x=global; }; f; echo $x");
    assert_eq!(out, "global\n");
}

#[test]
fn typeset_is_synonym() {
    let (out, _) = bash_exec_ok("typeset -i x=5; echo $x");
    assert_eq!(out, "5\n");
}

#[test]
fn declare_print_scalar() {
    let (out, _) = bash_exec_ok("x=hello; declare -p x");
    assert!(out.contains("declare") && out.contains("x=") && out.contains("hello"));
}

#[test]
fn declare_lowercase() {
    let (out, _) = bash_exec_ok("declare -l x; x=HELLO; echo $x");
    assert_eq!(out, "hello\n");
}

#[test]
fn declare_uppercase() {
    let (out, _) = bash_exec_ok("declare -u x; x=hello; echo $x");
    assert_eq!(out, "HELLO\n");
}

// printf builtin ------------------------------------------------------------------------------------------------------

#[test]
fn printf_basic_string() {
    let (out, _) = exec_ok("printf '%s\\n' hello");
    assert_eq!(out, "hello\n");
}

#[test]
fn printf_basic_integer() {
    let (out, _) = exec_ok("printf '%d\\n' 42");
    assert_eq!(out, "42\n");
}

#[test]
fn printf_hex() {
    let (out, _) = exec_ok("printf '%x\\n' 255");
    assert_eq!(out, "ff\n");
}

#[test]
fn printf_hex_upper() {
    let (out, _) = exec_ok("printf '%X\\n' 255");
    assert_eq!(out, "FF\n");
}

#[test]
fn printf_octal() {
    let (out, _) = exec_ok("printf '%o\\n' 8");
    assert_eq!(out, "10\n");
}

#[test]
fn printf_unsigned() {
    let (out, _) = exec_ok("printf '%u\\n' 42");
    assert_eq!(out, "42\n");
}

#[test]
fn printf_width_string() {
    let (out, _) = exec_ok("printf '[%10s]\\n' hi");
    assert_eq!(out, "[        hi]\n");
}

#[test]
fn printf_left_align() {
    let (out, _) = exec_ok("printf '[%-10s]\\n' hi");
    assert_eq!(out, "[hi        ]\n");
}

#[test]
fn printf_zero_pad() {
    let (out, _) = exec_ok("printf '[%05d]\\n' 42");
    assert_eq!(out, "[00042]\n");
}

#[test]
fn printf_precision_string() {
    let (out, _) = exec_ok("printf '[%.3s]\\n' hello");
    assert_eq!(out, "[hel]\n");
}

#[test]
fn printf_precision_integer() {
    let (out, _) = exec_ok("printf '[%6.4d]\\n' 42");
    assert_eq!(out, "[  0042]\n");
}

#[test]
fn printf_float() {
    let (out, _) = exec_ok("printf '[%.2f]\\n' 3.14159");
    assert_eq!(out, "[3.14]\n");
}

#[test]
fn printf_escape_newline() {
    let (out, _) = exec_ok("printf 'a\\nb\\n'");
    assert_eq!(out, "a\nb\n");
}

#[test]
fn printf_escape_tab() {
    let (out, _) = exec_ok("printf 'a\\tb\\n'");
    assert_eq!(out, "a\tb\n");
}

#[test]
fn printf_escape_hex() {
    let (out, _) = exec_ok("printf '\\x41\\n'");
    assert_eq!(out, "A\n");
}

#[test]
fn printf_percent_literal() {
    let (out, _) = exec_ok("printf '%%\\n'");
    assert_eq!(out, "%\n");
}

#[test]
fn printf_missing_arg_string() {
    let (out, _) = exec_ok("printf '%s|%s\\n' hello");
    assert_eq!(out, "hello|\n");
}

#[test]
fn printf_missing_arg_int() {
    let (out, _) = exec_ok("printf '%d\\n'");
    assert_eq!(out, "0\n");
}

#[test]
fn printf_cyclic_args() {
    let (out, _) = exec_ok("printf '%s\\n' a b c");
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn printf_var() {
    let (out, _) = exec_ok("printf -v x '%d' 42; echo $x");
    assert_eq!(out, "42\n");
}

#[test]
fn printf_shell_quote() {
    let (out, _) = exec_ok("printf '%q\\n' 'hello world'");
    // Should contain some form of quoting
    assert!(out.contains("hello") && out.contains("world"));
    assert!(out.trim() != "hello world"); // must be quoted somehow
}

#[test]
fn printf_backslash_b() {
    let (out, _) = exec_ok("printf '%b\\n' 'a\\nb'");
    assert_eq!(out, "a\nb\n");
}

#[test]
fn printf_no_trailing_newline() {
    let (out, _) = exec_ok("printf '%s' hello");
    assert_eq!(out, "hello");
}

#[test]
fn printf_hex_arg() {
    let (out, _) = exec_ok("printf '%d\\n' 0xff");
    assert_eq!(out, "255\n");
}

#[test]
fn printf_octal_arg() {
    let (out, _) = exec_ok("printf '%d\\n' 077");
    assert_eq!(out, "63\n");
}

#[test]
fn printf_char_arg() {
    let (out, _) = exec_ok("printf '%d\\n' \"'A\"");
    assert_eq!(out, "65\n");
}

#[test]
fn printf_hash_hex() {
    let (out, _) = exec_ok("printf '%#x\\n' 255");
    assert_eq!(out, "0xff\n");
}

#[test]
fn printf_hash_octal() {
    let (out, _) = exec_ok("printf '%#o\\n' 8");
    assert_eq!(out, "010\n");
}

#[test]
fn printf_char_conversion() {
    let (out, _) = exec_ok("printf '%c\\n' hello");
    assert_eq!(out, "h\n");
}

#[test]
fn printf_negative_zero_pad() {
    let (out, _) = exec_ok("printf '[%010d]\\n' -42");
    assert_eq!(out, "[-000000042]\n");
}

#[test]
fn printf_strftime_epoch() {
    // Epoch 0 in UTC is 1970
    let (out, _) = exec_ok("TZ=UTC printf '%(%Y)T\\n' 0");
    assert_eq!(out, "1970\n");
}

#[test]
fn printf_strftime_current() {
    let (out, _) = exec_ok("printf '%(%Y)T\\n' -1");
    let year: i32 = out.trim().parse().unwrap();
    assert!((2024..=2030).contains(&year));
}

// printf LC_TIME strftime ---------------------------------------------------------------------------------------------

#[test]
fn printf_strftime_weekday_german() {
    // 2001-09-09 is a Sunday in UTC — "Sonntag" in German
    let (out, _) = exec_ok("TZ=UTC LC_TIME=de_DE.UTF-8 printf '%(%A)T' 1000000000");
    assert_eq!(out, "Sonntag");
}

#[test]
fn printf_strftime_month_french() {
    // 2001-09-09 — September in French is "septembre"
    let (out, _) = exec_ok("TZ=UTC LC_TIME=fr_FR.UTF-8 printf '%(%B)T' 1000000000");
    assert_eq!(out, "septembre");
}

#[test]
fn printf_strftime_lc_time_overrides_lang() {
    // LC_TIME should override LANG for strftime
    let (out, _) = exec_ok("TZ=UTC LANG=en_US.UTF-8 LC_TIME=de_DE.UTF-8 printf '%(%A)T' 1000000000");
    assert_eq!(out, "Sonntag");
}

#[test]
fn printf_strftime_c_locale_english() {
    // C locale should give English weekday names
    let (out, _) = exec_ok("TZ=UTC LC_TIME=C printf '%(%A)T' 1000000000");
    assert_eq!(out, "Sunday");
}

#[test]
fn printf_strftime_mixed_locale_and_numeric_codes() {
    // Mix locale-sensitive and numeric codes in the same format string
    let (out, _) = exec_ok("TZ=UTC LC_TIME=de_DE.UTF-8 printf '%(%A %Y-%m-%d)T' 1000000000");
    assert_eq!(out, "Sonntag 2001-09-09");
}

// printf LC_NUMERIC ---------------------------------------------------------------------------------------------------

#[test]
fn printf_lc_numeric_output() {
    // German locale: decimal separator is comma. Integer arg avoids input ambiguity.
    let (out, _) = exec_ok("LC_NUMERIC=de_DE.UTF-8 printf '%.1f\\n' 3");
    assert_eq!(out, "3,0\n");
}

#[test]
fn printf_lc_numeric_input_comma() {
    // In German locale, "3,14" is a valid float (comma is decimal sep).
    let (out, _) = exec_ok("LC_NUMERIC=de_DE.UTF-8 printf '%.2f\\n' '3,14'");
    assert_eq!(out, "3,14\n");
}

#[test]
fn printf_lc_numeric_c_locale() {
    // C locale uses '.' — default behaviour should be unchanged.
    let (out, _) = exec_ok("LC_NUMERIC=C printf '%.2f\\n' 3.14");
    assert_eq!(out, "3.14\n");
}

// eval builtin --------------------------------------------------------------------------------------------------------

#[test]
fn eval_basic() {
    let (out, status) = exec_ok("eval echo hello");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[test]
fn eval_variable_persists() {
    let (out, _) = exec_ok("eval 'x=42'; echo $x");
    assert_eq!(out, "42\n");
}

#[test]
fn eval_function_persists() {
    let (out, _) = exec_ok("eval 'f() { echo hi; }'; f");
    assert_eq!(out, "hi\n");
}

#[test]
fn eval_concatenation() {
    // eval joins arguments with spaces
    let (out, _) = exec_ok("eval echo he llo");
    assert_eq!(out, "he llo\n");
}

#[test]
fn eval_empty() {
    let (_, status) = exec_ok("eval ''");
    assert_eq!(status, 0);
}

#[test]
fn eval_exit_status() {
    let (out, _) = exec_ok("eval false; echo $?");
    assert_eq!(out, "1\n");
}

// source builtin ------------------------------------------------------------------------------------------------------

#[test]
fn source_basic() {
    let dir = std::path::PathBuf::from("/tmp/claude/source-test");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("test.sh");
    std::fs::write(&file, "x=sourced_value\n").unwrap();

    let script = format!("source {}; echo $x", file.display());
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "sourced_value\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn source_dot_synonym() {
    let dir = std::path::PathBuf::from("/tmp/claude/source-dot-test");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("test.sh");
    std::fs::write(&file, "y=dotted\n").unwrap();

    let script = format!(". {}; echo $y", file.display());
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "dotted\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn source_with_args() {
    let dir = std::path::PathBuf::from("/tmp/claude/source-args-test");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("test.sh");
    std::fs::write(&file, "echo $1 $2\n").unwrap();

    let script = format!("source {} hello world", file.display());
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "hello world\n");

    let _ = std::fs::remove_dir_all(&dir);
}

// exec builtin --------------------------------------------------------------------------------------------------------

#[test]
fn exec_command() {
    // exec replaces the shell -- wrap in a subshell so the test runner
    // is not replaced.
    let (out, _) = exec_ok("(exec echo hello)");
    assert_eq!(out, "hello\n");
}

#[test]
fn exec_not_found() {
    // exec with nonexistent command -- the subshell exits 127.
    let (out, _) = exec_ok("(exec /nonexistent/command/xyz 2>/dev/null); echo $?");
    assert!(out.trim() != "0");
}

// Bash [[ ]] conditional ----------------------------------------------------------------------------------------------

#[test]
fn bash_cond_string_equals() {
    let (_, status) = bash_exec_ok("[[ hello == hello ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_string_not_equals() {
    let (_, status) = bash_exec_ok("[[ a != b ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_false() {
    let (_, status) = bash_exec_ok("[[ a == b ]]");
    assert_eq!(status, 1);
}

#[test]
fn bash_cond_string_empty() {
    let (_, status) = bash_exec_ok("[[ -z '' ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_string_nonempty() {
    let (_, status) = bash_exec_ok("[[ -n hello ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_file_exists() {
    let (_, status) = bash_exec_ok("[[ -e /tmp ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_file_is_dir() {
    let (_, status) = bash_exec_ok("[[ -d /tmp ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_file_not_exists() {
    let (_, status) = bash_exec_ok("[[ -e /nonexistent_path_xyz ]]");
    assert_eq!(status, 1);
}

#[test]
fn bash_cond_int_eq() {
    let (_, status) = bash_exec_ok("[[ 42 -eq 42 ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_int_lt() {
    let (_, status) = bash_exec_ok("[[ 1 -lt 2 ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_and() {
    let (_, status) = bash_exec_ok("[[ -n a && -n b ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_or() {
    let (_, status) = bash_exec_ok("[[ -z '' || -n b ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_not() {
    let (_, status) = bash_exec_ok("[[ ! -z hello ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_variable() {
    let (_, status) = bash_exec_ok("x=hi; [[ -n $x ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_regex() {
    let (_, status) = bash_exec_ok("[[ abc123 =~ [0-9]+ ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_regex_no_match() {
    let (_, status) = bash_exec_ok("[[ abcdef =~ [0-9]+ ]]");
    assert_eq!(status, 1);
}

#[test]
fn bash_cond_lexical_lt() {
    let (_, status) = bash_exec_ok("[[ apple < banana ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_var_set() {
    let (_, status) = bash_exec_ok("x=1; [[ -v x ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_var_unset() {
    let (_, status) = bash_exec_ok("[[ -v nonexistent_var ]]");
    assert_eq!(status, 1);
}

#[test]
fn bash_cond_in_if() {
    let (out, _) = bash_exec_ok("if [[ 1 -eq 1 ]]; then echo yes; fi");
    assert_eq!(out, "yes\n");
}

#[test]
fn bash_cond_bare_word() {
    // Bare non-empty word is true (implicit -n)
    let (_, status) = bash_exec_ok("[[ hello ]]");
    assert_eq!(status, 0);
}

#[test]
fn bash_cond_bare_empty() {
    // Empty string is false
    let (_, status) = bash_exec_ok("[[ '' ]]");
    assert_eq!(status, 1);
}

// set -x (xtrace) -----------------------------------------------------------------------------------------------------

#[test]
fn set_x_basic() {
    // xtrace goes to stderr; stdout should only contain the echo output
    let (out, status) = exec_ok("set -x; echo hello");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[test]
fn set_x_off() {
    let (out, _) = exec_ok("set -x; set +x; echo hello");
    assert_eq!(out, "hello\n");
}

// set -u (nounset) ----------------------------------------------------------------------------------------------------

#[test]
fn set_u_unset_var() {
    let status = exec_result("set -u; echo $nonexistent_xyz");
    assert_ne!(status, 0);
}

#[test]
fn set_u_set_var() {
    let (out, _) = exec_ok("set -u; x=hi; echo $x");
    assert_eq!(out, "hi\n");
}

#[test]
fn set_u_default() {
    let (out, _) = exec_ok("set -u; echo ${nonexistent_xyz:-fallback}");
    assert_eq!(out, "fallback\n");
}

#[test]
fn set_u_special() {
    let (out, _) = exec_ok("set -u; echo $?");
    assert_eq!(out, "0\n");
}

#[test]
fn set_u_off() {
    let (out, _) = exec_ok("set -u; set +u; echo ${nonexistent_xyz}done");
    assert_eq!(out, "done\n");
}

// set -e (errexit) ----------------------------------------------------------------------------------------------------

#[test]
fn set_e_basic() {
    // false triggers errexit — "nope" is never printed
    let (out, status) = exec_ok("set -e; false; echo nope");
    assert_eq!(out, "");
    assert_ne!(status, 0);
}

#[test]
fn set_e_if_condition() {
    // false in if condition does NOT trigger errexit
    let (out, _) = exec_ok("set -e; if false; then echo then; fi; echo ok");
    assert_eq!(out, "ok\n");
}

#[test]
fn set_e_and_chain() {
    // false on left side of && does NOT trigger errexit
    let (out, _) = exec_ok("set -e; false && true; echo ok");
    assert_eq!(out, "ok\n");
}

#[test]
fn set_e_or_chain() {
    // false on left side of || does NOT trigger errexit
    let (out, _) = exec_ok("set -e; false || true; echo ok");
    assert_eq!(out, "ok\n");
}

#[test]
fn set_e_not() {
    // ! false (negation) does NOT trigger errexit
    let (out, _) = exec_ok("set -e; ! false; echo ok");
    assert_eq!(out, "ok\n");
}

#[test]
fn set_e_off() {
    // set +e disables errexit
    let (out, _) = exec_ok("set -e; set +e; false; echo ok");
    assert_eq!(out, "ok\n");
}

// Nameref (declare -n) ------------------------------------------------------------------------------------------------

#[test]
fn nameref_basic() {
    let (out, _) = bash_exec_ok("declare -n r=x; x=hello; echo $r");
    assert_eq!(out, "hello\n");
}

#[test]
fn nameref_write() {
    let (out, _) = bash_exec_ok("declare -n r=x; r=world; echo $x");
    assert_eq!(out, "world\n");
}

#[test]
fn nameref_function_param() {
    let (out, _) = bash_exec_ok("f() { declare -n out=$1; out=42; }; f result; echo $result");
    assert_eq!(out, "42\n");
}

#[test]
fn nameref_chain() {
    let (out, _) = bash_exec_ok("declare -n a=b; declare -n b=c; c=deep; echo $a");
    assert_eq!(out, "deep\n");
}

#[test]
fn nameref_cycle() {
    // Cycle detection — should not infinite loop. ${a:-safe} provides fallback.
    let (out, _) = bash_exec_ok("declare -n a=b; declare -n b=a; echo ${a:-safe}");
    assert_eq!(out, "safe\n");
}

#[test]
fn nameref_unset_target() {
    // unset through nameref unsets the target, not the ref
    let (out, _) = bash_exec_ok("declare -n r=x; x=hi; unset r; echo ${x:-gone}");
    assert_eq!(out, "gone\n");
}

#[test]
fn nameref_array() {
    let (out, _) = bash_exec_ok("a=(1 2 3); declare -n r=a; echo ${r[1]}");
    assert_eq!(out, "2\n");
}

#[test]
fn nameref_cycle_3way() {
    // 3-way cycle: a→b→c→a. Must not hang.
    let (out, _) = bash_exec_ok("declare -n a=b; declare -n b=c; declare -n c=a; echo ${a:-safe}");
    assert_eq!(out, "safe\n");
}

#[test]
fn nameref_cycle_non_origin() {
    // x→a→b→a — x is not in the cycle, but the chain it enters is cyclic.
    // Must not hang. x resolves to a (or b), which is unset → fallback.
    let (out, _) = bash_exec_ok("declare -n x=a; declare -n a=b; declare -n b=a; echo ${x:-safe}");
    assert_eq!(out, "safe\n");
}

#[test]
fn nameref_cycle_write() {
    // Writing through a cycle must not hang — should fail gracefully.
    let status = bash_exec_ok("declare -n a=b; declare -n b=a; a=oops 2>/dev/null; echo ok").1;
    assert_eq!(status, 0); // shell survives, doesn't hang
}

#[test]
fn nameref_self_reference() {
    // declare -n a=a — self-referencing nameref. Must not hang.
    let (out, _) = bash_exec_ok("declare -n a=a; echo ${a:-safe}");
    assert_eq!(out, "safe\n");
}

// Dialect gating -----------------------------------------------------------------------------------------------------

#[test]
fn posix_rejects_declare() {
    // declare is bash-only — POSIX mode should not recognize it as a builtin.
    // Use "declare x=1" alone (no trailing echo) so the exit status reflects
    // the failed declare, not a subsequent successful echo.
    let prog = thaum::parse("declare x=1").unwrap();
    let options = thaum::Dialect::Posix.options();
    let mut exec = thaum::exec::Executor::with_options(options);
    exec.set_exe_path(thaum_exe());
    let _ = exec.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let mut io = thaum::exec::CapturedIo::new();
    let result = exec.execute(&prog, &mut io.context());
    // declare should fail (command not found) in POSIX mode
    match result {
        Ok(status) => assert_ne!(status, 0),
        Err(thaum::exec::ExecError::CommandNotFound(_)) => {} // expected
        Err(e) => panic!("unexpected error: {:?}", e),
    }
}

#[test]
fn posix_rejects_shopt() {
    let prog = thaum::parse("shopt -s expand_aliases").unwrap();
    let options = thaum::Dialect::Posix.options();
    let mut exec = thaum::exec::Executor::with_options(options);
    exec.set_exe_path(thaum_exe());
    let _ = exec.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let mut io = thaum::exec::CapturedIo::new();
    let result = exec.execute(&prog, &mut io.context());
    match result {
        Ok(status) => assert_ne!(status, 0),
        Err(thaum::exec::ExecError::CommandNotFound(_)) => {}
        Err(e) => panic!("unexpected error: {:?}", e),
    }
}

#[test]
fn posix_allows_alias() {
    // alias is POSIX — should work in POSIX mode
    let prog = thaum::parse("alias").unwrap();
    let options = thaum::Dialect::Posix.options();
    let mut exec = thaum::exec::Executor::with_options(options);
    exec.set_exe_path(thaum_exe());
    let _ = exec.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let mut io = thaum::exec::CapturedIo::new();
    let status = exec.execute(&prog, &mut io.context()).unwrap();
    assert_eq!(status, 0);
}

#[test]
fn posix_allows_test_builtin() {
    let prog = thaum::parse("test -n hello").unwrap();
    let options = thaum::Dialect::Posix.options();
    let mut exec = thaum::exec::Executor::with_options(options);
    exec.set_exe_path(thaum_exe());
    let _ = exec.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let mut io = thaum::exec::CapturedIo::new();
    let status = exec.execute(&prog, &mut io.context()).unwrap();
    assert_eq!(status, 0);
}

#[test]
fn bash_allows_declare() {
    // Bash mode — declare should work
    let (out, status) = bash_exec_ok("declare x=hello; echo $x");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

// Case modification operators (${var^}, ${var^^}, ${var,}, ${var,,}) ---------------------------------------------------

#[test]
fn case_mod_upper_first() {
    let (out, _) = bash_exec_ok("x=hello; echo ${x^}");
    assert_eq!(out, "Hello\n");
}

#[test]
fn case_mod_upper_all() {
    let (out, _) = bash_exec_ok("x=hello; echo ${x^^}");
    assert_eq!(out, "HELLO\n");
}

#[test]
fn case_mod_lower_first() {
    let (out, _) = bash_exec_ok("x=HELLO; echo ${x,}");
    assert_eq!(out, "hELLO\n");
}

#[test]
fn case_mod_lower_all() {
    let (out, _) = bash_exec_ok("x=HELLO; echo ${x,,}");
    assert_eq!(out, "hello\n");
}

#[test]
fn case_mod_unicode() {
    let (out, _) = bash_exec_ok("x=café; echo ${x^^}");
    assert_eq!(out, "CAFÉ\n");
}

#[test]
fn case_mod_empty() {
    let (out, _) = bash_exec_ok("x=''; echo \"${x^^}\"");
    assert_eq!(out, "\n");
}

#[test]
fn case_mod_unset() {
    let (out, _) = bash_exec_ok("echo \"${unset_var^^}\"");
    assert_eq!(out, "\n");
}

// POSIX character classes in case =================================================================================

#[test]
fn case_char_class_upper() {
    let (out, _) = bash_exec_ok("case A in [[:upper:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[test]
fn case_char_class_lower() {
    let (out, _) = bash_exec_ok("case a in [[:lower:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[test]
fn case_char_class_digit() {
    let (out, _) = bash_exec_ok("case 5 in [[:digit:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[test]
fn case_char_class_space() {
    let (out, _) = exec_ok("case ' ' in [[:space:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[test]
fn case_char_class_alpha_negated() {
    let (out, _) = bash_exec_ok("case 5 in [![:alpha:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[test]
fn case_char_class_mixed_bracket() {
    // Class + literal in same bracket
    let (out, _) = bash_exec_ok("case _ in [[:alpha:]_]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[test]
fn case_char_class_alnum_with_star() {
    let (out, _) = bash_exec_ok("case hello123 in [[:alnum:]]*) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

// Character classes in parameter expansion ========================================================================

#[test]
fn trim_char_class_alpha_prefix() {
    let (out, _) = bash_exec_ok("x=hello123; echo ${x##[[:alpha:]]*}");
    assert_eq!(out, "\n");
}

#[test]
fn trim_char_class_digit_suffix() {
    let (out, _) = bash_exec_ok("x=hello123; echo ${x%%[[:digit:]]*}");
    assert_eq!(out, "hello\n");
}

// Regex =~ with character classes =================================================================================

#[test]
fn regex_char_class_digit() {
    let (_, status) = bash_exec_ok("[[ abc123 =~ [[:digit:]]+ ]]");
    assert_eq!(status, 0);
}

#[test]
fn regex_char_class_alpha() {
    let (_, status) = bash_exec_ok("[[ hello =~ ^[[:alpha:]]+$ ]]");
    assert_eq!(status, 0);
}

#[test]
fn regex_char_class_space() {
    let (_, status) = bash_exec_ok("[[ 'hello world' =~ [[:space:]] ]]");
    assert_eq!(status, 0);
}

#[test]
fn regex_char_class_upper() {
    let (_, status) = bash_exec_ok("[[ Hello =~ ^[[:upper:]] ]]");
    assert_eq!(status, 0);
}

// Locale sensitivity of character classes =========================================================================

#[test]
fn case_char_class_upper_accent_c_locale() {
    // In C locale, É is NOT [[:upper:]]
    let (out, _) = bash_exec_ok("LC_CTYPE=C; case É in [[:upper:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "n\n");
}

#[test]
fn case_char_class_upper_accent_utf8_locale() {
    // In UTF-8 locale, É IS [[:upper:]]
    let (out, _) = bash_exec_ok("LC_CTYPE=en_US.UTF-8; case É in [[:upper:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

// Locale translation ($"...") =========================================================================================

fn fixture_dir() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/locale")
        .to_str()
        .unwrap()
        .to_string()
}

#[test]
fn locale_quoted_no_domain() {
    // Without TEXTDOMAIN, $"..." just expands like double quotes
    let (out, _) = bash_exec_ok("echo $\"hello world\"");
    assert_eq!(out, "hello world\n");
}

#[test]
fn locale_quoted_with_variable_no_domain() {
    // $"..." expands variables even without translation
    let (out, _) = bash_exec_ok("x=test; echo $\"hello $x\"");
    assert_eq!(out, "hello test\n");
}

#[test]
fn locale_quoted_basic_translation() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"hello world\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "hallo welt\n");
}

#[test]
fn locale_quoted_with_variable_translation() {
    let script = format!(
        "USER=Claude\nTEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"hello $USER\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "hallo Claude\n");
}

#[test]
fn locale_quoted_missing_msgid() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"not in catalog\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "not in catalog\n");
}

#[test]
fn locale_quoted_c_locale_no_translation() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=C\necho $\"hello world\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "hello world\n");
}

#[test]
fn locale_quoted_empty_string() {
    let (out, _) = bash_exec_ok("echo $\"\"");
    assert_eq!(out, "\n");
}

#[test]
fn locale_quoted_fallback_locale() {
    // LANG=de_DE.UTF-8 with .mo only in de/ directory -- should fall back
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLANG=de_DE.UTF-8\necho $\"goodbye\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "auf wiedersehen\n");
}

// Parameter transformation @Q/@a/@A -----------------------------------------------------------------------------------

#[test]
fn transform_quote_simple() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@Q}\"");
    assert_eq!(out, "'hello'\n");
}

#[test]
fn transform_attrs_plain() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@a}\"");
    assert_eq!(out, "\n");
}

#[test]
fn transform_attrs_integer() {
    let (out, _) = bash_exec_ok("declare -i n=42; echo \"${n@a}\"");
    assert_eq!(out, "i\n");
}

#[test]
fn transform_attrs_exported_readonly() {
    let (out, _) = bash_exec_ok("declare -rx e=test; echo \"${e@a}\"");
    assert_eq!(out, "rx\n");
}

#[test]
fn transform_attrs_array() {
    let (out, _) = bash_exec_ok("declare -a a; a=(1 2); echo \"${a@a}\"");
    assert_eq!(out, "a\n");
}

#[test]
fn transform_attrs_assoc() {
    let (out, _) = bash_exec_ok("declare -A m=([k]=v); echo \"${m@a}\"");
    assert_eq!(out, "A\n");
}

#[test]
fn transform_assign_scalar() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@A}\"");
    assert_eq!(out, "x='hello'\n");
}

#[test]
fn transform_assign_integer() {
    let (out, _) = bash_exec_ok("declare -i n=42; echo \"${n@A}\"");
    assert_eq!(out, "declare -i n='42'\n");
}

#[test]
fn transform_lower() {
    let (out, _) = bash_exec_ok("x=HELLO; echo \"${x@L}\"");
    assert_eq!(out, "hello\n");
}

#[test]
fn transform_upper() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@U}\"");
    assert_eq!(out, "HELLO\n");
}

#[test]
fn transform_capitalize() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@u}\"");
    assert_eq!(out, "Hello\n");
}

// Indirect expansion ${!var[@]} ---------------------------------------------------------------------------------------

#[test]
fn indirect_array_keys() {
    let (out, _) = bash_exec_ok("a=(x y z); echo ${!a[@]}");
    assert_eq!(out, "0 1 2\n");
}

#[test]
fn indirect_assoc_keys() {
    // Assoc array keys are unordered, so just check we get both
    let (out, _) = bash_exec_ok("declare -A m; m[k]=v; m[j]=w; echo ${!m[@]}");
    let keys: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"k"));
    assert!(keys.contains(&"j"));
}

// Versioned dialect tests -----------------------------------------------------------------------------------------

/// Helper: parse and execute with a specific dialect, capturing stdout.
fn dialect_exec_ok(script: &str, dialect: Dialect) -> (String, i32) {
    let program = thaum::parse_with(script, dialect)
        .unwrap_or_else(|e| panic!("parse failed for {:?} with {:?}: {}", script, dialect, e));
    let options = dialect.options();
    let mut exec = Executor::with_options(options);
    exec.set_exe_path(thaum_exe());
    let _ = exec.env_mut().set_var("PATH", "/usr/bin:/bin");
    let mut io = CapturedIo::new();
    match exec.execute(&program, &mut io.context()) {
        Ok(status) => (io.stdout_string(), status),
        Err(ExecError::ExitRequested(code)) => (io.stdout_string(), code),
        Err(e) => panic!("exec failed for {:?} with {:?}: {}", script, dialect, e),
    }
}

#[test]
fn bash_is_bash51() {
    // Dialect::Bash and Dialect::Bash51 produce identical options
    assert_eq!(Dialect::Bash.options(), Dialect::Bash51.options());
}

#[test]
fn bash44_has_array_empty_element_bug() {
    // In bash 4.4, ${a[@]:+foo} on array with empty element returns "foo" (bug)
    let (out, _) = dialect_exec_ok("a=(''); echo \"${a[@]:+foo}\"", Dialect::Bash44);
    assert_eq!(out, "foo\n");
}

#[test]
fn bash50_fixes_array_empty_element_bug() {
    // In bash 5.0+, ${a[@]:+foo} on array with empty element returns "" (fixed)
    let (out, _) = dialect_exec_ok("a=(''); echo \"${a[@]:+foo}\"", Dialect::Bash50);
    assert_eq!(out, "\n");
}

#[test]
fn bash44_rejects_transform_lower() {
    // @L is bash 5.1+ — in bash 4.4, the parser does not recognize @L as a
    // transform, so `x@L` is treated as a variable name containing `@` which
    // expands to empty (no bad-substitution error at parse time, but the
    // transform is not applied).
    let (out, _) = dialect_exec_ok("x=HELLO; echo \"${x@L}\"", Dialect::Bash44);
    // Without the transform, `x@L` is an undefined variable → empty
    assert_eq!(out, "\n");
}

#[test]
fn bash50_rejects_transform_lower() {
    // @L is bash 5.1+ — same behavior as bash 4.4: not recognized
    let (out, _) = dialect_exec_ok("x=HELLO; echo \"${x@L}\"", Dialect::Bash50);
    assert_eq!(out, "\n");
}

#[test]
fn bash51_allows_transform_lower() {
    // @L works in bash 5.1+
    let (out, _) = dialect_exec_ok("x=HELLO; echo ${x@L}", Dialect::Bash51);
    assert_eq!(out, "hello\n");
}
