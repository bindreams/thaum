//! Tests for external command stdout/stderr capture and the test_tools fixture.

use std::path::Path;

use crate::*;

// External command output capture =====================================================================================

#[skuld::test]
fn external_echo_captured(#[fixture(test_tools)] tools: &Path) {
    let (out, _err, status) = exec_with_tools("echo hello", tools);
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[skuld::test]
fn external_echo_with_args(#[fixture(test_tools)] tools: &Path) {
    let (out, _, status) = exec_with_tools("echo a b c", tools);
    assert_eq!(status, 0);
    assert_eq!(out, "a b c\n");
}

#[skuld::test]
fn external_stderr_captured(#[fixture(test_tools)] tools: &Path) {
    let (out, err, status) = exec_with_tools("sh -c 'echo err >&2'", tools);
    assert_eq!(status, 0);
    assert_eq!(out, "");
    assert_eq!(err, "err\n");
}

#[skuld::test]
fn external_both_streams_captured(#[fixture(test_tools)] tools: &Path) {
    let (out, err, status) = exec_with_tools("sh -c 'echo out; echo err >&2'", tools);
    assert_eq!(status, 0);
    assert_eq!(out, "out\n");
    assert_eq!(err, "err\n");
}

#[skuld::test]
fn external_large_stderr_no_deadlock(#[fixture(test_tools)] tools: &Path) {
    // Generate >64KB on stderr + stdout to stress-test concurrent pipe reading.
    // The pipe buffer is typically 64KB on Linux; if reads are sequential, this deadlocks.
    let script =
        "sh -c 'i=0; while [ $i -lt 2000 ]; do echo stdout_line_$i; echo stderr_line_$i >&2; i=$((i+1)); done'";
    let (out, err, status) = exec_with_tools(script, tools);
    assert_eq!(status, 0);
    // Verify we got output on both streams (exact count depends on buffering).
    assert!(
        out.lines().count() >= 1000,
        "expected >=1000 stdout lines, got {}",
        out.lines().count()
    );
    assert!(
        err.lines().count() >= 1000,
        "expected >=1000 stderr lines, got {}",
        err.lines().count()
    );
}

#[skuld::test]
fn external_exit_status(#[fixture(test_tools)] tools: &Path) {
    let (_, _, status) = exec_with_tools("true", tools);
    assert_eq!(status, 0);
    let (_, _, status) = exec_with_tools("false", tools);
    assert_eq!(status, 1);
}

#[skuld::test]
fn external_not_found(#[fixture(test_tools)] tools: &Path) {
    let (_, err, status) = exec_with_tools("nonexistent_command_xyz_123", tools);
    assert_eq!(status, 127);
    assert!(
        err.contains("command not found"),
        "stderr should mention 'command not found', got: {err}"
    );
}

#[skuld::test]
fn external_with_redirect_bypasses_pipe(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let file = dir.join("stdout.txt");
    let f = file.to_string_lossy().replace('\\', "/");
    let script = format!("echo hello > {f}");
    let (out, _, status) = exec_with_tools(&script, tools);
    assert_eq!(status, 0);
    assert_eq!(out, "", "stdout should be empty when redirected to file");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

// cat through pipeline ------------------------------------------------------------------------------------------------

#[skuld::test]
fn external_cat_in_pipeline(#[fixture(test_tools)] tools: &Path) {
    let (out, _, status) = exec_with_tools("echo hello | cat", tools);
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

// Non-capturing I/O (live mode) ---------------------------------------------------------------------------------------

/// When the IoContext is non-capturing, external commands should inherit parent
/// handles directly. Their output bypasses the IoContext buffer entirely.
#[skuld::test]
fn external_non_capturing_inherits_handles(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    let program = thaum::parse("env").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    // Export a variable so `env` produces output.
    let _ = executor.env_mut().set_var("THAUM_TEST_VAR", "1");
    executor.env_mut().export_var("THAUM_TEST_VAR");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.capturing = false;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    // In non-capturing mode, external commands inherit parent handles.
    // The CapturedIo buffer should be empty because output went to
    // the real process stdout, not through IoContext.
    assert_eq!(
        captured.stdout_string(),
        "",
        "non-capturing IoContext should not receive external command output"
    );
}

/// Regression guard: capturing mode continues to capture external command output.
#[skuld::test]
fn external_capturing_captures_output(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    let program = thaum::parse("env").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    let _ = executor.env_mut().set_var("THAUM_TEST_VAR", "1");
    executor.env_mut().export_var("THAUM_TEST_VAR");

    let mut captured = CapturedIo::new();
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(status, 0);
    assert!(
        captured.stdout_string().contains("THAUM_TEST_VAR=1"),
        "capturing IoContext should receive external command output, got: {:?}",
        captured.stdout_string()
    );
}

// sh -c tests (thaum sh impersonation) --------------------------------------------------------------------------------

#[skuld::test]
fn sh_dash_c_basic(#[fixture(test_tools)] tools: &Path) {
    let (out, _, status) = exec_with_tools("sh -c 'echo hello from sh'", tools);
    assert_eq!(status, 0);
    assert_eq!(out, "hello from sh\n");
}

#[skuld::test]
fn sh_dash_c_exit_status(#[fixture(test_tools)] tools: &Path) {
    let (_, _, status) = exec_with_tools("sh -c 'exit 42'", tools);
    assert_eq!(status, 42);
}
