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
