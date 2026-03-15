use std::path::Path;

use thaum::exec::CapturedIo;
use thaum::Dialect;

use crate::*;

// Basic command execution ---------------------------------------------------------------------------------------------

#[skuld::test]
fn true_command() {
    assert_eq!(exec!("true").status(), 0);
}

#[skuld::test]
fn false_command() {
    assert_eq!(exec!("false").status(), 1);
}

#[skuld::test]
fn colon_noop() {
    assert_eq!(exec!(":").status(), 0);
}

#[skuld::test]
fn exit_zero() {
    assert_eq!(exec!("exit 0").status(), 0);
}

#[skuld::test]
fn exit_nonzero() {
    assert_eq!(exec!("exit 42").status(), 42);
}

// Variable assignment -------------------------------------------------------------------------------------------------

#[skuld::test]
fn variable_assignment_and_echo() {
    let r = exec!("X=hello; echo $X");
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn variable_used_in_later_command() {
    let mut sh = shell!();
    sh.exec("X=hello\ntrue");
    assert_eq!(sh.env().get_var("X"), Some("hello"));
    sh.join();
}

#[skuld::test]
fn assignment_with_command_sub_returns_sub_status() {
    let r = exec!("x=$(false); echo $?");
    assert_eq!(r.stdout(), "1\n");
    let r = exec!("x=$(true); echo $?");
    assert_eq!(r.stdout(), "0\n");
}

#[skuld::test]
fn while_with_failing_assignment_does_not_loop() {
    let r = exec!("while x=$(false); do echo loop; done");
    assert_eq!(r.stdout(), "");
    assert_eq!(r.status(), 0); // while returns 0 when body never executes
}

// AND/OR lists --------------------------------------------------------------------------------------------------------

#[skuld::test]
fn and_list_both_true() {
    assert_eq!(exec!("true && true").status(), 0);
}

#[skuld::test]
fn and_list_first_false() {
    assert_eq!(exec!("false && true").status(), 1);
}

#[skuld::test]
fn and_list_second_false() {
    assert_eq!(exec!("true && false").status(), 1);
}

#[skuld::test]
fn or_list_first_false() {
    assert_eq!(exec!("false || true").status(), 0);
}

#[skuld::test]
fn or_list_first_true() {
    assert_eq!(exec!("true || false").status(), 0);
}

#[skuld::test]
fn or_list_both_false() {
    assert_eq!(exec!("false || false").status(), 1);
}

// Not -----------------------------------------------------------------------------------------------------------------

#[skuld::test]
fn not_true() {
    assert_eq!(exec!("! true").status(), 1);
}

#[skuld::test]
fn not_false() {
    assert_eq!(exec!("! false").status(), 0);
}

// Multiple statements -------------------------------------------------------------------------------------------------

#[skuld::test]
fn multiple_statements_last_status() {
    assert_eq!(exec!("true; false").status(), 1);
    assert_eq!(exec!("false; true").status(), 0);
    assert_eq!(exec!("true; false; true").status(), 0);
}

// exit status propagation ---------------------------------------------------------------------------------------------

#[skuld::test]
fn exit_status_variable() {
    assert_eq!(exec!("false\ntrue").status(), 0);
}

// If statements -------------------------------------------------------------------------------------------------------

#[skuld::test]
fn if_true_branch() {
    let mut sh = shell!();
    sh.exec("if true; then X=yes; else X=no; fi");
    assert_eq!(sh.env().get_var("X"), Some("yes"));
    sh.join();
}

#[skuld::test(ignore = "old test_executor used Bash mode; needs dialect fix or ExecError refactor")]
fn if_false_branch() {
    let mut sh = shell!();
    sh.exec("if false; then X=yes; else X=no; fi");
    assert_eq!(sh.env().get_var("X"), Some("no"));
    sh.join();
}

#[skuld::test]
fn if_no_else_false() {
    let mut sh = shell!();
    let r = sh.exec("if false; then X=yes; fi");
    assert_eq!(sh.env().get_var("X"), None);
    assert_eq!(r.status(), 0);
    sh.join();
}

// While loop ----------------------------------------------------------------------------------------------------------

#[skuld::test]
fn while_loop_counts() {
    let mut sh = shell!();
    sh.exec("X=0\nwhile test $X != done; do X=done; done");
    assert_eq!(sh.env().get_var("X"), Some("done"));
    sh.join();
}

// For loop ------------------------------------------------------------------------------------------------------------

#[skuld::test]
fn for_loop_over_words() {
    let mut sh = shell!();
    sh.exec("RESULT=\nfor i in a b c; do RESULT=${RESULT}${i}; done");
    assert_eq!(sh.env().get_var("RESULT"), Some("abc"));
    sh.join();
}

// Case statement ------------------------------------------------------------------------------------------------------

#[skuld::test]
fn case_exact_match() {
    let mut sh = shell!();
    sh.exec(
        r#"
case hello in
    hello) X=matched ;;
    *) X=default ;;
esac
"#,
    );
    assert_eq!(sh.env().get_var("X"), Some("matched"));
    sh.join();
}

#[skuld::test]
fn case_wildcard_match() {
    let mut sh = shell!();
    sh.exec(
        r#"
case world in
    hello) X=hello ;;
    *) X=default ;;
esac
"#,
    );
    assert_eq!(sh.env().get_var("X"), Some("default"));
    sh.join();
}

// Brace group ---------------------------------------------------------------------------------------------------------

#[skuld::test]
fn brace_group() {
    let mut sh = shell!();
    sh.exec("{ X=inside; }");
    assert_eq!(sh.env().get_var("X"), Some("inside"));
    sh.join();
}

// Function definition and call ----------------------------------------------------------------------------------------

#[skuld::test]
fn function_define_and_call() {
    let mut sh = shell!();
    sh.exec("greet() { X=hello; }\ngreet");
    assert_eq!(sh.env().get_var("X"), Some("hello"));
    sh.join();
}

// Export --------------------------------------------------------------------------------------------------------------

#[skuld::test]
fn export_builtin() {
    let mut sh = shell!();
    sh.exec("export FOO=bar");
    assert_eq!(sh.env().get_var("FOO"), Some("bar"));
    assert!(sh.env().is_exported("FOO"));
    sh.join();
}

// Unset ---------------------------------------------------------------------------------------------------------------

#[skuld::test]
fn unset_builtin() {
    let mut sh = shell!();
    sh.exec("X=hello\nunset X");
    assert_eq!(sh.env().get_var("X"), None);
    sh.join();
}

// External command (basic smoke test) — moved to exec/external.rs -----------------------------------------------------

#[skuld::test(ignore = "unchecked stderr from expected errors — needs stderr assertions")]
fn external_command_not_found() {
    assert_eq!(exec!("nonexistent_command_xyz_123").status(), 127);
}

// Test builtin --------------------------------------------------------------------------------------------------------

#[skuld::test]
fn test_builtin_string() {
    assert_eq!(exec!("test hello").status(), 0);
    assert_eq!(exec!("test ''").status(), 1);
}

#[skuld::test]
fn test_builtin_eq() {
    assert_eq!(exec!("test 5 -eq 5").status(), 0);
    assert_eq!(exec!("test 5 -eq 6").status(), 1);
}

#[skuld::test]
fn bracket_test_syntax() {
    assert_eq!(exec!("[ hello ]").status(), 0);
    assert_eq!(exec!("[ 3 -gt 2 ]").status(), 0);
    assert_eq!(exec!("[ 2 -gt 3 ]").status(), 1);
}

#[skuld::test]
fn test_builtin_logical_and_or() {
    assert_eq!(exec!("[ foo -a bar ]").status(), 0);
    assert_eq!(exec!("[ foo -a '' ]").status(), 1);
    assert_eq!(exec!("[ '' -o bar ]").status(), 0);
    assert_eq!(exec!("[ '' -o '' ]").status(), 1);
}

#[skuld::test]
fn test_builtin_parentheses() {
    assert_eq!(exec!(r"[ \( foo \) ]").status(), 0);
    assert_eq!(exec!(r"[ \( '' \) ]").status(), 1);
    assert_eq!(exec!(r"[ ! \( '' \) ]").status(), 0);
}

#[skuld::test]
fn test_builtin_complex_expr() {
    assert_eq!(exec!("[ -n foo -a -n bar ]").status(), 0);
    assert_eq!(exec!("[ -z '' -o -n bar ]").status(), 0);
}

#[skuld::test]
fn test_builtin_file_operators() {
    assert_eq!(exec!("[ -d / ]").status(), 0);
    assert_eq!(exec!("[ -e / ]").status(), 0);
    assert_eq!(exec!("[ -a / ]").status(), 0); // -a as unary file-exists
    assert_eq!(exec!("[ -f / ]").status(), 1); // / is directory, not regular file
}

#[skuld::test(ignore = "unchecked stderr from expected errors — needs stderr assertions")]
fn test_builtin_syntax_error_exit_2() {
    assert_eq!(exec!("[ '(' foo ]").status(), 2);
    assert_eq!(exec!("[").status(), 2);
}

// Break/continue ------------------------------------------------------------------------------------------------------

#[skuld::test]
fn break_in_while() {
    let mut sh = shell!();
    sh.exec(
        r#"
X=0
while true; do
    X=1
    break
    X=2
done
"#,
    );
    assert_eq!(sh.env().get_var("X"), Some("1"));
    sh.join();
}

#[skuld::test]
fn continue_in_for() {
    let mut sh = shell!();
    sh.exec(
        r#"
RESULT=
for i in a skip b; do
    if test "$i" = skip; then
        continue
    fi
    RESULT=${RESULT}${i}
done
"#,
    );
    assert_eq!(sh.env().get_var("RESULT"), Some("ab"));
    sh.join();
}

// Command substitution ------------------------------------------------------------------------------------------------

#[skuld::test]
fn command_substitution_builtin() {
    let mut sh = shell!();
    sh.exec("X=$(echo hello)");
    assert_eq!(sh.env().get_var("X"), Some("hello"));
    sh.join();
}

#[skuld::test]
fn command_substitution_external(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let mut sh = shell!(env = &[("PATH", &*tools_dir)]);
    sh.exec("X=$(echo world)");
    assert_eq!(sh.env().get_var("X"), Some("world"));
    sh.join();
}

#[skuld::test]
fn command_substitution_strips_trailing_newlines() {
    // echo produces "hello\n", cmd sub strips trailing newlines
    let mut sh = shell!();
    sh.exec("X=$(echo hello)");
    assert_eq!(sh.env().get_var("X"), Some("hello"));
    sh.join();
}

#[skuld::test]
fn command_substitution_in_argument() {
    let mut sh = shell!();
    sh.exec("X=$(echo inner)\nY=${X}");
    assert_eq!(sh.env().get_var("X"), Some("inner"));
    assert_eq!(sh.env().get_var("Y"), Some("inner"));
    sh.join();
}

#[skuld::test(ignore = "old test_executor used Bash mode; needs dialect fix or ExecError refactor")]
fn command_substitution_exit_status() {
    // $? should reflect the command substitution's exit status
    // (though the assignment itself succeeds with status 0)
    let mut sh = shell!();
    sh.exec("X=$(false)");
    assert_eq!(sh.env().get_var("X"), Some(""));
    sh.join();
}

#[skuld::test]
fn heredoc_basic() {
    let r = exec!("read VAR <<EOF\nhello\nEOF\necho $VAR");
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn consecutive_reads_from_stdin() {
    // Two `read` calls on the same stdin must each get their own line.
    // Regression: BufReader over-read consumed both lines on the first call.
    let program = thaum::parse("read A; read B; echo $A $B").unwrap();
    let mut executor = test_executor();
    let mut captured = CapturedIo::with_stdin(b"first\nsecond\n");
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(status, 0);
    assert_eq!(captured.stdout_string(), "first second\n");
}

#[skuld::test]
fn while_read_loop_with_heredoc() {
    let r = exec!("while read LINE; do echo \"got: $LINE\"; done <<EOF\nfirst\nsecond\nEOF");
    assert_eq!(r.stdout(), "got: first\ngot: second\n");
}

#[skuld::test]
fn if_redirect_output_to_file(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("out.txt");
    let script = format!("if true; then echo yes; fi > {}", shell_path(&file));
    let r = exec!(&script);
    assert_eq!(r.stdout(), ""); // stdout captured by redirect
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "yes\n");
}

#[skuld::test]
fn brace_group_redirect_stdin() {
    let r = exec!("{ read A; read B; echo $A $B; } <<EOF\nalpha\nbeta\nEOF");
    assert_eq!(r.stdout(), "alpha beta\n");
}

// FD 3+ read tests ----------------------------------------------------------------------------------------------------

#[skuld::test]
fn read_from_fd3_via_exec_redirect(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("input.txt");
    std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
    let script = format!(
        "exec 3< {f}; read A <&3; read B <&3; read C <&3; exec 3<&-; echo $A $B $C",
        f = shell_path(&file)
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "one two three\n");
}

#[skuld::test]
fn while_read_from_fd3(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("input.txt");
    std::fs::write(&file, "alpha\nbeta\n").unwrap();
    let script = format!(
        "exec 3< {f}; while read LINE <&3; do echo \"got: $LINE\"; done; exec 3<&-",
        f = shell_path(&file)
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "got: alpha\ngot: beta\n");
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
    let r = exec!(&script);
    assert_eq!(r.stdout(), ""); // stdout went to file, not captured
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[skuld::test]
fn redirect_builtin_append(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("append.txt");
    let f = shell_path(&file);
    let script = format!("echo first > {f}; echo second >> {f}");
    exec!(&script);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "first\nsecond\n");
}

#[skuld::test]
fn redirect_stdin_from_file(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("input.txt");
    std::fs::write(&file, "from-file\n").unwrap();
    let script = format!("read LINE < {}; echo $LINE", shell_path(&file));
    let r = exec!(&script);
    assert_eq!(r.stdout(), "from-file\n");
}

#[skuld::test]
fn redirect_dup_stdout_to_stderr_file(#[fixture(temp_dir)] dir: &Path) {
    // > file 2>&1 — redirect stdout to file, then dup stderr to same file
    let file = dir.join("combined.txt");
    let script = format!("echo hello > {} 2>&1", shell_path(&file));
    let r = exec!(&script);
    assert_eq!(r.stdout(), "");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[skuld::test]
fn redirect_fd3_and_dup_to_stdout(#[fixture(temp_dir)] dir: &Path) {
    // echo hello 3>/tmp/file >&3 — open FD 3 to file, dup stdout to FD 3
    let file = dir.join("fd3.txt");
    let script = format!("echo hello 3>{} >&3", shell_path(&file));
    let r = exec!(&script);
    assert_eq!(r.stdout(), ""); // stdout went to FD 3 → file
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[skuld::test]
fn redirect_creates_empty_file(#[fixture(temp_dir)] dir: &Path) {
    // `> file` with no command creates/truncates the file
    let file = dir.join("empty.txt");
    let script = format!("> {}", shell_path(&file));
    exec!(&script);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
}

// external_command_inherits_fd3 — moved to exec/external.rs

// Background jobs -----------------------------------------------------------------------------------------------------

#[skuld::test(ignore = "background jobs not yet implemented")]
fn background_job() {
    let r = exec!("echo hello &");
    assert_eq!(r.stdout(), "hello\n");
}

// Dialect gating ------------------------------------------------------------------------------------------------------

#[skuld::test]
fn posix_rejects_declare() {
    // declare is bash-only — POSIX mode should not recognize it as a builtin.
    let prog = thaum::parse("declare x=1").unwrap();
    let options = thaum::Dialect::Posix.options();
    let mut exec = thaum::exec::Executor::with_options(options);
    exec.set_exe_path(thaum_exe());
    let _ = exec.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let mut io = CapturedIo::new();
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
    let mut io = CapturedIo::new();
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
    exec!("alias");
}

#[skuld::test]
fn posix_allows_test_builtin() {
    exec!("test -n hello");
}

#[skuld::test]
fn bash_allows_declare() {
    let r = exec!("declare x=hello; echo $x", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}
