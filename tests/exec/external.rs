//! Tests for external command stdout/stderr capture and the test_tools fixture.

use std::path::Path;

use crate::*;

// External command output capture =====================================================================================

#[skuld::test]
fn external_echo_captured(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("echo hello", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn external_echo_with_args(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("echo a b c", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.stdout(), "a b c\n");
}

#[skuld::test]
fn external_stderr_captured(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("sh -c 'echo err >&2'", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.stdout(), "");
    assert_eq!(r.stderr(), "err\n");
}

#[skuld::test]
fn external_both_streams_captured(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("sh -c 'echo out; echo err >&2'", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.stdout(), "out\n");
    assert_eq!(r.stderr(), "err\n");
}

#[skuld::test]
fn external_large_stderr_no_deadlock(#[fixture(test_tools)] tools: &Path) {
    // Generate >64KB on stderr + stdout to stress-test concurrent pipe reading.
    // The pipe buffer is typically 64KB on Linux; if reads are sequential, this deadlocks.
    let script =
        "sh -c 'i=0; while [ $i -lt 2000 ]; do echo stdout_line_$i; echo stderr_line_$i >&2; i=$((i+1)); done'";
    let tools_dir = tools.to_string_lossy();
    let r = exec!(script, env = &[("PATH", &*tools_dir)]);
    let out = r.stdout();
    let err = r.stderr();
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
    let tools_dir = tools.to_string_lossy();
    let r = exec!("true", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.status(), 0);
    let r = exec!("false", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.status(), 1);
}

#[skuld::test]
fn external_not_found(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("nonexistent_command_xyz_123", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.status(), 127);
    let err = r.stderr();
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
    let tools_dir = tools.to_string_lossy();
    let r = exec!(&*script, env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.stdout(), "", "stdout should be empty when redirected to file");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

// cat through pipeline ------------------------------------------------------------------------------------------------

#[skuld::test]
fn external_cat_in_pipeline(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("echo hello | cat", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.stdout(), "hello\n");
}

/// Pipeline with a nonexistent command should report exit code 127 and print an error.
#[skuld::test]
fn pipeline_command_not_found_status(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("echo hello | nonexistent_cmd", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.status(), 127);
    assert!(r.stderr().contains("command not found"));
}

/// When both tty flags are true, the last pipeline stage uses ConPTY on Windows.
/// This exercises the drain_and_wait_conpty + subsequent wait() path, which
/// previously triggered undefined behavior (double-wait on closed handles).
#[skuld::test]
fn pipeline_last_stage_conpty_no_double_wait(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    // `isatty` doesn't read stdin, avoiding deadlocks from ConPTY stdin override.
    let program = thaum::parse("echo hello | isatty").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = true;
    ctx.tty_stderr = true;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    let out = captured.stdout_string();
    // On Windows with ConPTY, the child should see stdout as a TTY.
    // On Unix with Pty, same. The key assertion is no crash/UB from double-wait.
    assert!(!out.is_empty(), "pipeline should produce output from isatty, got empty");
}

/// Piped stdin must reach a ConPTY child. Without PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
/// the pipe handle isn't inherited and `cat` reads from ConPTY's empty console input.
#[skuld::test]
fn pipeline_stdin_reaches_conpty_child(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    let program = thaum::parse("echo hello | cat").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = true;
    ctx.tty_stderr = true;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    let out = captured.stdout_string();
    assert!(
        out.contains("hello"),
        "piped stdin should reach ConPTY child, got: {out:?}"
    );
}

/// File redirections for fds 3+ must work alongside ConPTY. Without
/// PROC_THREAD_ATTRIBUTE_HANDLE_LIST, `bInheritHandles=false` prevents the
/// fd 3 handle from being inherited.
#[skuld::test]
fn external_conpty_with_fd_redirect(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    use thaum::exec::CapturedIo;

    let file = dir.join("fd3.txt");
    let f = file.to_string_lossy().replace('\\', "/");
    let script = format!("echo hello 3> {f}");
    let program = thaum::parse(&script).unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = true;
    ctx.tty_stderr = true;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    assert!(file.exists(), "fd 3 redirect should create the file");
}

/// Mid-pipeline stages should see stdout as a pipe, not a PTY, even when the
/// shell's tty_stdout is true. Only the last stage gets the PTY.
#[skuld::test]
fn pipeline_mid_stage_stdout_is_pipe(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    // `isatty` is the first stage — its stdout is piped to `cat`.
    let program = thaum::parse("isatty | cat").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = true;
    ctx.tty_stderr = true;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    let out = captured.stdout_string();
    assert!(
        out.contains("stdout:no"),
        "mid-pipeline stage stdout should be a pipe, not a PTY, got: {out:?}"
    );
}

// PATH resolution (genuinely external commands) -----------------------------------------------------------------------

/// `env` is NOT a builtin — this exercises real PATH resolution + process spawning.
/// On Windows, this validates that resolve_command finds `env.exe` via PATH.
#[skuld::test]
fn external_env_found_via_path(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("env", env = &[("PATH", &*tools_dir)]);
    // env prints exported vars; the test executor exports `_`, so at minimum we get `_=env`.
    assert!(!r.stdout().is_empty(), "env should produce output, got empty");
}

/// External command output is relayed through IoContext to the caller's buffers.
#[skuld::test]
fn external_output_relayed_through_io_context(#[fixture(test_tools)] tools: &Path) {
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
        "IoContext should receive external command output, got: {:?}",
        captured.stdout_string()
    );
}

// PTY forwarding ------------------------------------------------------------------------------------------------------

/// When both tty_stdout and tty_stderr are true, the child should see a terminal
/// on fd 1 (via ConPTY on Windows, PTY on Unix).
#[skuld::test]
fn external_pty_stdout_reports_tty(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    let program = thaum::parse("isatty").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = true;
    ctx.tty_stderr = true;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    let out = captured.stdout_string();
    assert!(
        out.contains("stdout:yes"),
        "child should see stdout as TTY when tty_stdout=true, got: {out:?}"
    );
    assert!(
        !out.contains('\r'),
        "PTY output should not contain \\r after normalization, got: {out:?}"
    );
}

/// When only stdout requests a TTY but stderr does not, the child should still
/// see stdout as a terminal. POSIX semantics: isatty() is independent per fd.
#[skuld::test]
fn external_mixed_tty_stdout_only(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    let program = thaum::parse("isatty").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = true;
    ctx.tty_stderr = false;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    let out = captured.stdout_string();
    assert!(
        out.contains("stdout:yes"),
        "stdout should be TTY even when stderr is not, got: {out:?}"
    );
    assert!(
        out.contains("stderr:no"),
        "stderr should be pipe when tty_stderr=false, got: {out:?}"
    );
    assert!(
        !out.contains('\r'),
        "PTY output should not contain \\r after normalization, got: {out:?}"
    );
}

/// When only stderr requests a TTY but stdout does not, the child should still
/// see stderr as a terminal. Symmetric case of mixed_tty_stdout_only.
#[skuld::test]
fn external_mixed_tty_stderr_only(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    let program = thaum::parse("isatty").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = false;
    ctx.tty_stderr = true;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    let out = captured.stdout_string();
    assert!(
        out.contains("stdout:no"),
        "stdout should be pipe when tty_stdout=false, got: {out:?}"
    );
    assert!(
        out.contains("stderr:yes"),
        "stderr should be TTY even when stdout is not, got: {out:?}"
    );
    assert!(
        !out.contains('\r'),
        "PTY output should not contain \\r after normalization, got: {out:?}"
    );
}

/// PTY output should have clean \n line endings, not \r\n. Both Unix PTYs (ONLCR)
/// and Windows ConPTY convert \n → \r\n by default; the shell must normalize this.
#[skuld::test]
fn external_pty_output_no_crlf(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    let program = thaum::parse("echo hello").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = true;
    ctx.tty_stderr = true;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);
    let out = captured.stdout_string();
    assert!(
        out.contains("hello"),
        "PTY echo should produce output containing 'hello', got: {out:?}"
    );
    assert!(
        !out.contains('\r'),
        "PTY output should use \\n not \\r\\n, got: {out:?}"
    );
}

/// When tty_stdout is false (default for CapturedIo), the child should see a pipe.
#[skuld::test]
fn external_pipe_stdout_reports_no_tty(#[fixture(test_tools)] tools: &Path) {
    use thaum::exec::CapturedIo;

    let program = thaum::parse("isatty").unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let status = executor.execute(&program, &mut captured.context()).unwrap();
    assert_eq!(status, 0);
    let out = captured.stdout_string();
    assert!(
        out.contains("stdout:no"),
        "child should see stdout as pipe when tty_stdout=false, got: {out:?}"
    );
}

/// When stdout is redirected to a file, the tty flag is cleared even if the parent's
/// stdout is a terminal. The child should see a pipe, not a TTY.
#[skuld::test]
fn external_redirect_clears_tty(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    use thaum::exec::CapturedIo;

    let file = dir.join("out.txt");
    let f = file.to_string_lossy().replace('\\', "/");
    let script = format!("isatty > {f}");
    let program = thaum::parse(&script).unwrap();
    let mut executor = crate::test_executor_with_tools(tools);
    executor.env_mut().export_var("PATH");

    let mut captured = CapturedIo::new();
    let mut ctx = captured.context();
    ctx.tty_stdout = true;

    let status = executor.execute(&program, &mut ctx).unwrap();
    assert_eq!(status, 0);

    let output = std::fs::read_to_string(&file).unwrap();
    assert!(
        output.contains("stdout:no"),
        "redirected stdout should NOT be a TTY, got: {output:?}"
    );
}

// sh -c tests (thaum sh impersonation) --------------------------------------------------------------------------------

#[skuld::test]
fn sh_dash_c_basic(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("sh -c 'echo hello from sh'", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.stdout(), "hello from sh\n");
}

#[skuld::test]
fn sh_dash_c_exit_status(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("sh -c 'exit 42'", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.status(), 42);
}
