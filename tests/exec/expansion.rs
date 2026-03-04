use std::path::Path;
use thaum::exec::{CapturedIo, ExecError, Executor};
use thaum::Dialect;

use crate::*;

// Arithmetic expansion $((expr)) --------------------------------------------------------------------------------------

#[skuld::test]
fn arith_expansion_simple() {
    let program = thaum::parse("X=$((1+2))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("3"));
}

#[skuld::test]
fn arith_expansion_with_variables() {
    let program = thaum::parse("A=10\nX=$((A+5))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("15"));
}

#[skuld::test]
fn arith_expansion_in_double_quotes() {
    let program = thaum::parse(r#"X="val: $((2*3))""#).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("val: 6"));
}

#[skuld::test]
fn arith_expansion_with_assignment_side_effect() {
    let program = thaum::parse("X=$((y=5))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("5"));
    assert_eq!(executor.env().get_var("y"), Some("5"));
}

#[skuld::test]
fn arith_expansion_division_by_zero() {
    let program = thaum::parse("X=$((1/0))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let err = executor.execute(&program, &mut captured.context()).unwrap_err();
    assert!(matches!(err, ExecError::DivisionByZero));
}

#[skuld::test]
fn arith_expansion_nested_ops() {
    let program = thaum::parse("X=$(( (2 + 3) * 4 ))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("20"));
}

#[skuld::test]
fn arith_expansion_unset_var_is_zero() {
    let program = thaum::parse("X=$((UNSET + 1))").unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("X"), Some("1"));
}

// Bash (( )) arithmetic command ---------------------------------------------------------------------------------------

#[skuld::test]
fn bash_arith_command_nonzero_is_success() {
    let program = thaum::parse_with("(( 5 ))", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    assert_eq!(executor.execute(&program, &mut captured.context()).unwrap(), 0);
}

#[skuld::test]
fn bash_arith_command_zero_is_failure() {
    let program = thaum::parse_with("(( 0 ))", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    assert_eq!(executor.execute(&program, &mut captured.context()).unwrap(), 1);
}

#[skuld::test]
fn bash_arith_command_with_assignment() {
    let program = thaum::parse_with("(( x = 42 ))", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(status, 0); // 42 != 0 → success
    assert_eq!(executor.env().get_var("x"), Some("42"));
}

// Bash for (( )) arithmetic for loop ----------------------------------------------------------------------------------

#[skuld::test]
fn bash_arith_for_basic() {
    let program = thaum::parse_with("for ((i=0; i<5; i++)); do true; done", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("i"), Some("5"));
}

#[skuld::test]
fn bash_arith_for_sum() {
    let program = thaum::parse_with("sum=0\nfor ((i=1; i<=10; i++)); do sum=$((sum+i)); done", Dialect::Bash).unwrap();
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(executor.env().get_var("sum"), Some("55"));
}

#[skuld::test]
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

// DefaultAssign (${var:=default}) -------------------------------------------------------------------------------------

#[skuld::test]
fn default_assign_when_unset() {
    let (out, status) = exec_ok("echo ${X:=hello}; echo $X");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\nhello\n");
}

#[skuld::test]
fn default_assign_when_set() {
    let (out, status) = exec_ok("X=existing; echo ${X:=fallback}; echo $X");
    assert_eq!(status, 0);
    assert_eq!(out, "existing\nexisting\n");
}

// Pattern trimming ----------------------------------------------------------------------------------------------------

#[skuld::test]
fn trim_small_suffix() {
    let (out, _) = exec_ok("X=hello.txt; echo ${X%.txt}");
    assert_eq!(out, "hello\n");
}

#[skuld::test]
fn trim_large_suffix() {
    let (out, _) = exec_ok("X=archive.tar.gz; echo ${X%%.*}");
    assert_eq!(out, "archive\n");
}

#[skuld::test]
fn trim_small_prefix() {
    let (out, _) = exec_ok("X=/usr/bin:/usr/local/bin; echo ${X#*/}");
    assert_eq!(out, "usr/bin:/usr/local/bin\n");
}

#[skuld::test]
fn trim_large_prefix() {
    // ${X##*/} extracts basename
    let (out, _) = exec_ok("X=/a/b/c.txt; echo ${X##*/}");
    assert_eq!(out, "c.txt\n");
}

// readonly builtin ----------------------------------------------------------------------------------------------------

#[skuld::test]
fn readonly_set_and_read() {
    let (out, status) = exec_ok("readonly X=42; echo $X");
    assert_eq!(status, 0);
    assert_eq!(out, "42\n");
}

#[skuld::test]
fn readonly_prevents_assignment() {
    let status = exec_result("readonly X=42; X=99");
    assert_ne!(status, 0);
}

// local builtin -------------------------------------------------------------------------------------------------------

#[skuld::test]
fn local_scopes_variable_in_function() {
    let (out, _) = exec_ok("f() { local X=inner; echo $X; }; X=outer; f; echo $X");
    assert_eq!(out, "inner\nouter\n");
}

#[skuld::test]
fn local_unset_var_removed_on_exit() {
    let (out, _) = exec_ok("f() { local Y=temp; echo $Y; }; f; echo \"${Y:-gone}\"");
    assert_eq!(out, "temp\ngone\n");
}

#[skuld::test]
fn local_outside_function_fails() {
    let status = exec_result("local X=1");
    assert_ne!(status, 0);
}

// eval builtin --------------------------------------------------------------------------------------------------------

#[skuld::test]
fn eval_basic() {
    let (out, status) = exec_ok("eval echo hello");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[skuld::test]
fn eval_variable_persists() {
    let (out, _) = exec_ok("eval 'x=42'; echo $x");
    assert_eq!(out, "42\n");
}

#[skuld::test]
fn eval_function_persists() {
    let (out, _) = exec_ok("eval 'f() { echo hi; }'; f");
    assert_eq!(out, "hi\n");
}

#[skuld::test]
fn eval_concatenation() {
    // eval joins arguments with spaces
    let (out, _) = exec_ok("eval echo he llo");
    assert_eq!(out, "he llo\n");
}

#[skuld::test]
fn eval_empty() {
    let (_, status) = exec_ok("eval ''");
    assert_eq!(status, 0);
}

#[skuld::test]
fn eval_exit_status() {
    let (out, _) = exec_ok("eval false; echo $?");
    assert_eq!(out, "1\n");
}

// source builtin ------------------------------------------------------------------------------------------------------

/// Convert a path to a forward-slash string suitable for embedding in shell scripts.
fn shell_path(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[skuld::test]
fn source_basic(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("test.sh");
    std::fs::write(&file, "x=sourced_value\n").unwrap();

    let script = format!("source {}; echo $x", shell_path(&file));
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "sourced_value\n");
}

#[skuld::test]
fn source_dot_synonym(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("test.sh");
    std::fs::write(&file, "y=dotted\n").unwrap();

    let script = format!(". {}; echo $y", shell_path(&file));
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "dotted\n");
}

#[skuld::test]
fn source_with_args(#[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("test.sh");
    std::fs::write(&file, "echo $1 $2\n").unwrap();

    let script = format!("source {} hello world", shell_path(&file));
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "hello world\n");
}

#[skuld::test]
fn source_finds_script_via_path_lookup(#[fixture(temp_dir)] dir: &Path) {
    // Put a script in a temp directory, add that directory to PATH,
    // and source by bare name (no slashes) to exercise find_in_path().
    let script_path = dir.join("my_sourceable.sh");
    std::fs::write(&script_path, "sourced_via_path=yes\n").unwrap();

    let dir_str = shell_path(dir);
    // Use the platform's PATH separator so the test validates the fix on all platforms.
    let sep = if cfg!(windows) { ";" } else { ":" };
    let script = format!("PATH=\"{dir_str}{sep}/usr/bin{sep}/bin\"; source my_sourceable.sh; echo $sourced_via_path");
    let (out, _) = exec_ok(&script);
    assert_eq!(out, "yes\n");
}

// exec builtin --------------------------------------------------------------------------------------------------------

#[skuld::test]
fn exec_command(#[fixture(test_tools)] tools: &Path) {
    // exec replaces the shell — needs a real binary on PATH (not a builtin).
    let (out, _, _) = exec_with_tools("(exec echo hello)", tools);
    assert_eq!(out, "hello\n");
}

#[skuld::test]
fn exec_not_found() {
    // exec with nonexistent command -- the subshell exits 127.
    let (out, _) = exec_ok("(exec /nonexistent/command/xyz 2>/dev/null); echo $?");
    assert!(out.trim() != "0");
}

#[skuld::test]
fn exec_rejects_unknown_flags() {
    // Bash: `exec -z` → "exec: -z: invalid option", exit 2.
    let (out, _) = exec_ok("(exec -z 2>/dev/null); echo $?");
    assert_eq!(out.trim(), "2", "exec with unknown flag should exit 2");
    // Also verify that unknown flag is rejected even with a command following.
    assert_eq!(exec_status("(exec -z true 2>/dev/null)"), 2);
}

// exec redirect-only mode -----------------------------------------------------------------------------------------

#[skuld::test]
fn exec_redirect_fd3_persists(#[fixture(temp_dir)] dir: &Path) {
    // exec 3>file opens FD 3 for the rest of the shell session.
    let file = dir.join("fd3.txt");
    let f = shell_path(&file);

    let script = format!("exec 3>{f}; echo hello >&3; echo world >&3; exec 3>&-");
    let (_, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "hello\nworld\n",
        "FD 3 should persist across multiple writes"
    );
}

#[skuld::test]
fn exec_redirect_stdout_to_file(#[fixture(temp_dir)] dir: &Path) {
    // exec 1>file redirects stdout to a file for all subsequent commands.
    let file = dir.join("stdout.txt");
    let f = shell_path(&file);

    let script = format!("exec 1>{f}; echo redirected");
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, "", "captured stdout should be empty after exec 1>file");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "redirected\n");
}

#[skuld::test]
fn exec_redirect_affects_subshell(#[fixture(temp_dir)] dir: &Path) {
    // exec 1>file must redirect stdout for ALL subsequent commands,
    // including compound commands and subshells — not just simple commands.
    let file = dir.join("stdout.txt");
    let f = shell_path(&file);

    let script = format!("exec 1>{f}; (echo from_subshell)");
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, "", "subshell stdout should go to file, not captured");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "from_subshell\n");
}

#[skuld::test]
fn exec_redirect_affects_compound(#[fixture(temp_dir)] dir: &Path) {
    // exec 1>file should also apply to brace groups and if/while bodies.
    let file = dir.join("stdout.txt");
    let f = shell_path(&file);

    let script = format!("exec 1>{f}; if true; then echo from_if; fi");
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, "", "if-body stdout should go to file");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "from_if\n");
}

#[skuld::test]
fn exec_close_fd(#[fixture(temp_dir)] dir: &Path) {
    // exec 3>file; echo hello >&3; exec 3>&- closes FD 3.
    // Verify the file only contains writes from before the close.
    let file = dir.join("fd3.txt");
    let f = shell_path(&file);

    let script = format!("exec 3>{f}; echo hello >&3; exec 3>&-");
    let (_, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[skuld::test]
fn exec_with_redirect_to_file(#[fixture(test_tools)] tools: &Path) {
    // exec 2>/dev/null echo hello — redirects applied before exec.
    // The subshell's stderr is discarded; stdout should still work.
    let (out, _, _) = exec_with_tools("(exec 2>/dev/null echo hello)", tools);
    assert_eq!(out, "hello\n");
}

#[skuld::test]
fn exec_inherits_per_command_fds(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    // exec 3>file cmd — the per-command redirect should be applied before exec.
    let file = dir.join("fd3.txt");
    let f = shell_path(&file);

    let script = format!("(exec 3>{f} sh -c 'echo from_exec >&3')");
    let (_, _, status) = exec_with_tools(&script, tools);
    assert_eq!(status, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "from_exec\n");
}

#[skuld::test]
fn exec_fd3_inherited_by_subshell(#[fixture(temp_dir)] dir: &Path) {
    // exec 3>file persists FD 3, and a subshell should inherit it.
    let file = dir.join("fd3.txt");
    let f = shell_path(&file);

    let script = format!("exec 3>{f}; (echo hello >&3); exec 3>&-");
    let (_, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[skuld::test]
fn exec_closed_fd_not_inherited_by_subshell(#[fixture(temp_dir)] dir: &Path) {
    // After exec 3>&-, a subsequent subshell must NOT see FD 3.
    // This validates that fd_table is explicitly constructed per-spawn
    // (no CLOEXEC race conditions).
    let file = dir.join("fd3.txt");
    let f = shell_path(&file);

    // Open FD 3 and immediately close it. The subshell should fail to use FD 3.
    let script = format!("exec 3>{f}; echo before >&3; exec 3>&-; (echo after >&3 2>/dev/null; echo $?)");
    let (out, _) = exec_ok(&script);
    // The subshell's echo should fail; $? should be non-zero.
    assert!(
        out.trim() != "0",
        "closed FD 3 should not be inherited by subshell; got: {out}"
    );
    // The file should only have "before" from before the close.
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "before\n");
}

#[skuld::test]
fn exec_dash_a_sets_argv0(#[fixture(test_tools)] tools: &Path) {
    // exec -a custom_name uses a custom argv[0].
    // We verify by having the child print $0 (which reflects argv[0]).
    let (out, _, _) = exec_with_tools("(exec -a custom_name sh -c 'echo $0'); echo done", tools);
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines[0], "custom_name", "argv[0] should be 'custom_name'; got: {out}");
    assert_eq!(lines[1], "done", "parent should continue after subshell");
}
