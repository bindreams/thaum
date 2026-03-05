use std::path::Path;

use thaum::exec::{CapturedIo, Executor};

use crate::*;

// Basic command execution ---------------------------------------------------------------------------------------------

#[skuld::test]
fn true_command() {
    assert_eq!(exec_status("true"), 0);
}

#[skuld::test]
fn false_command() {
    assert_eq!(exec_status("false"), 1);
}

#[skuld::test]
fn colon_noop() {
    assert_eq!(exec_status(":"), 0);
}

#[skuld::test]
fn exit_zero() {
    assert_eq!(exec_status("exit 0"), 0);
}

#[skuld::test]
fn exit_nonzero() {
    assert_eq!(exec_status("exit 42"), 42);
}

// Variable assignment -------------------------------------------------------------------------------------------------

#[skuld::test]
fn variable_assignment_and_echo() {
    let (out, status) = exec_ok("X=hello; echo $X");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[skuld::test]
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

#[skuld::test]
fn and_list_both_true() {
    assert_eq!(exec_status("true && true"), 0);
}

#[skuld::test]
fn and_list_first_false() {
    assert_eq!(exec_status("false && true"), 1);
}

#[skuld::test]
fn and_list_second_false() {
    assert_eq!(exec_status("true && false"), 1);
}

#[skuld::test]
fn or_list_first_false() {
    assert_eq!(exec_status("false || true"), 0);
}

#[skuld::test]
fn or_list_first_true() {
    assert_eq!(exec_status("true || false"), 0);
}

#[skuld::test]
fn or_list_both_false() {
    assert_eq!(exec_status("false || false"), 1);
}

// Not -----------------------------------------------------------------------------------------------------------------

#[skuld::test]
fn not_true() {
    assert_eq!(exec_status("! true"), 1);
}

#[skuld::test]
fn not_false() {
    assert_eq!(exec_status("! false"), 0);
}

// Multiple statements -------------------------------------------------------------------------------------------------

#[skuld::test]
fn multiple_statements_last_status() {
    assert_eq!(exec_status("true; false"), 1);
    assert_eq!(exec_status("false; true"), 0);
    assert_eq!(exec_status("true; false; true"), 0);
}

// exit status propagation ---------------------------------------------------------------------------------------------

#[skuld::test]
fn exit_status_variable() {
    let program = thaum::parse("false\ntrue").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();

    // After executing, last exit status should be from `true` (0).
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(status, 0);
}

// If statements -------------------------------------------------------------------------------------------------------

#[skuld::test]
fn if_true_branch() {
    let program = thaum::parse("if true; then X=yes; else X=no; fi").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("yes"));
}

#[skuld::test]
fn if_false_branch() {
    let program = thaum::parse("if false; then X=yes; else X=no; fi").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("no"));
}

#[skuld::test]
fn if_no_else_false() {
    let program = thaum::parse("if false; then X=yes; fi").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), None);
    assert_eq!(status, 0);
}

// While loop ----------------------------------------------------------------------------------------------------------

#[skuld::test]
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

#[skuld::test]
fn for_loop_over_words() {
    let program = thaum::parse("RESULT=\nfor i in a b c; do RESULT=${RESULT}${i}; done").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("RESULT"), Some("abc"));
}

// Case statement ------------------------------------------------------------------------------------------------------

#[skuld::test]
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

#[skuld::test]
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

#[skuld::test]
fn brace_group() {
    let program = thaum::parse("{ X=inside; }").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("inside"));
}

// Function definition and call ----------------------------------------------------------------------------------------

#[skuld::test]
fn function_define_and_call() {
    let program = thaum::parse("greet() { X=hello; }\ngreet").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

// Export --------------------------------------------------------------------------------------------------------------

#[skuld::test]
fn export_builtin() {
    let program = thaum::parse("export FOO=bar").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("FOO"), Some("bar"));
    assert!(executor.env().is_exported("FOO"));
}

// Unset ---------------------------------------------------------------------------------------------------------------

#[skuld::test]
fn unset_builtin() {
    let program = thaum::parse("X=hello\nunset X").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), None);
}

// External command (basic smoke test) — moved to exec/external.rs -------------------------------------------------------

#[skuld::test]
fn external_command_not_found() {
    assert_eq!(exec_status("nonexistent_command_xyz_123"), 127);
}

// Test builtin --------------------------------------------------------------------------------------------------------

#[skuld::test]
fn test_builtin_string() {
    assert_eq!(exec_status("test hello"), 0);
    assert_eq!(exec_status("test ''"), 1);
}

#[skuld::test]
fn test_builtin_eq() {
    assert_eq!(exec_status("test 5 -eq 5"), 0);
    assert_eq!(exec_status("test 5 -eq 6"), 1);
}

#[skuld::test]
fn bracket_test_syntax() {
    assert_eq!(exec_status("[ hello ]"), 0);
    assert_eq!(exec_status("[ 3 -gt 2 ]"), 0);
    assert_eq!(exec_status("[ 2 -gt 3 ]"), 1);
}

#[skuld::test]
fn test_builtin_logical_and_or() {
    assert_eq!(exec_status("[ foo -a bar ]"), 0);
    assert_eq!(exec_status("[ foo -a '' ]"), 1);
    assert_eq!(exec_status("[ '' -o bar ]"), 0);
    assert_eq!(exec_status("[ '' -o '' ]"), 1);
}

#[skuld::test]
fn test_builtin_parentheses() {
    assert_eq!(exec_status(r"[ \( foo \) ]"), 0);
    assert_eq!(exec_status(r"[ \( '' \) ]"), 1);
    assert_eq!(exec_status(r"[ ! \( '' \) ]"), 0);
}

#[skuld::test]
fn test_builtin_complex_expr() {
    assert_eq!(exec_status("[ -n foo -a -n bar ]"), 0);
    assert_eq!(exec_status("[ -z '' -o -n bar ]"), 0);
}

#[skuld::test]
fn test_builtin_file_operators() {
    assert_eq!(exec_status("[ -d / ]"), 0);
    assert_eq!(exec_status("[ -e / ]"), 0);
    assert_eq!(exec_status("[ -a / ]"), 0); // -a as unary file-exists
    assert_eq!(exec_status("[ -f / ]"), 1); // / is directory, not regular file
}

#[skuld::test]
fn test_builtin_syntax_error_exit_2() {
    assert_eq!(exec_status("[ '(' foo ]"), 2);
    assert_eq!(exec_status("["), 2);
}

// Break/continue ------------------------------------------------------------------------------------------------------

#[skuld::test]
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

#[skuld::test]
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

#[skuld::test]
fn command_substitution_builtin() {
    let program = thaum::parse("X=$(echo hello)").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

#[skuld::test]
fn command_substitution_external(#[fixture(test_tools)] tools: &Path) {
    let program = thaum::parse("X=$(echo world)").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("world"));
}

#[skuld::test]
fn command_substitution_strips_trailing_newlines() {
    let program = thaum::parse("X=$(echo hello)").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    // echo produces "hello\n", cmd sub strips trailing newlines
    assert_eq!(executor.env().get_var("X"), Some("hello"));
}

#[skuld::test]
fn command_substitution_in_argument() {
    // Test that $(...) works in command arguments
    let program = thaum::parse("X=$(echo inner)\nY=${X}").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("inner"));
    assert_eq!(executor.env().get_var("Y"), Some("inner"));
}

#[skuld::test]
fn command_substitution_exit_status() {
    let program = thaum::parse("X=$(false)").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    // $? should reflect the command substitution's exit status
    // (though the assignment itself succeeds with status 0)
    assert_eq!(executor.env().get_var("X"), Some(""));
}

#[skuld::test]
fn heredoc_basic() {
    let (out, status) = exec_ok("read VAR <<EOF\nhello\nEOF\necho $VAR");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[skuld::test]
fn unsupported_compound_redirect() {
    expect_unsupported("if true; then echo hi; fi > /dev/null");
}

// Redirect tests ------------------------------------------------------------------------------------------------------

/// Convert a path to a forward-slash string suitable for embedding in shell scripts.
fn shell_path(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[skuld::test]
fn redirect_builtin_stdout_to_file(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("stdout.txt");

    let script = format!("echo hello > {}", shell_path(&file));
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, ""); // stdout went to file, not captured
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[skuld::test]
fn redirect_builtin_append(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("append.txt");
    let f = shell_path(&file);

    let script = format!("echo first > {f}; echo second >> {f}");
    let (_, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "first\nsecond\n");
}

#[skuld::test]
fn redirect_stdin_from_file(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("input.txt");
    std::fs::write(&file, "from-file\n").unwrap();

    let script = format!("read LINE < {}; echo $LINE", shell_path(&file));
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "from-file\n");
}

#[skuld::test]
fn redirect_dup_stdout_to_stderr_file(#[fixture(temp_dir)] dir: &Path) {
    // > file 2>&1 — redirect stdout to file, then dup stderr to same file
    let file = dir.join("combined.txt");

    let script = format!("echo hello > {} 2>&1", shell_path(&file));
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, "");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[skuld::test]
fn redirect_fd3_and_dup_to_stdout(#[fixture(temp_dir)] dir: &Path) {
    // echo hello 3>/tmp/file >&3 — open FD 3 to file, dup stdout to FD 3
    let file = dir.join("fd3.txt");

    let script = format!("echo hello 3>{} >&3", shell_path(&file));
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, ""); // stdout went to FD 3 → file
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[skuld::test]
fn redirect_creates_empty_file(#[fixture(temp_dir)] dir: &Path) {
    // `> file` with no command creates/truncates the file
    let file = dir.join("empty.txt");

    let script = format!("> {}", shell_path(&file));
    let (_, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
}

// external_command_inherits_fd3 — moved to exec/external.rs

// Unsupported features produce explicit errors ------------------------------------------------------------------------

#[skuld::test]
fn unsupported_background() {
    expect_unsupported("echo hello &");
}

// Dialect gating -----------------------------------------------------------------------------------------------------

#[skuld::test]
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
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[skuld::test]
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
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[skuld::test]
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

#[skuld::test]
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

#[skuld::test]
fn bash_allows_declare() {
    // Bash mode — declare should work
    let (out, status) = bash_exec_ok("declare x=hello; echo $x");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}
