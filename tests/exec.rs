//! Execution tests: verify runtime behavior of the thaum executor.

pub use skuld::temp_dir;
pub use thaum_test_tools::test_tools;

#[path = "common/mod.rs"]
mod common;

#[path = "exec/append.rs"]
mod append;
#[path = "exec/arrays.rs"]
mod arrays;
#[path = "exec/bash.rs"]
mod bash;
#[path = "exec/basic.rs"]
mod basic;
#[path = "exec/brace_expansion.rs"]
mod brace_expansion;
#[path = "exec/expansion.rs"]
mod expansion;
#[path = "exec/external.rs"]
mod external;
#[path = "exec/interactive.rs"]
mod interactive;
#[path = "exec/printf.rs"]
mod printf;
#[path = "exec/variables.rs"]
mod variables;

use thaum::exec::{CapturedIo, Environment, ExecError, Executor};
use thaum::Dialect;

fn main() {
    skuld::run_all();
}

skuld::default_labels!(lex, parse, exec);

/// Find the thaum binary for subshell tests.
///
/// During `cargo test`, the test binary is NOT the thaum CLI. We need the
/// actual `thaum` binary which lives at `target/debug/thaum` (or
/// `target/release/thaum`).
pub fn thaum_exe() -> std::path::PathBuf {
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
///
/// Uses a clean environment (no process inheritance) so tests are not affected
/// by the host's locale variables (`LC_ALL`, `LANG`, etc.).
pub fn test_executor() -> Executor {
    let mut env = Environment::new();
    let _ = env.set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let mut executor = Executor::with_env(env);
    executor.set_exe_path(thaum_exe());
    executor
}

/// Create a Bash-dialect executor for tests (includes Bash-specific variables).
pub fn test_bash_executor() -> Executor {
    let mut env = Environment::new();
    let _ = env.set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let options = Dialect::Bash.options();
    let mut executor = Executor::with_env_and_options(env, options);
    executor.set_exe_path(thaum_exe());
    executor
}

/// Parse and execute a script, capturing stdout. Returns (stdout, exit_status).
pub fn exec_ok(script: &str) -> (String, i32) {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));

    let mut executor = test_executor();

    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => (captured.stdout_string(), status),
        Err(ExecError::ExitRequested(code)) => (captured.stdout_string(), code),
        Err(e) => panic!("exec failed for {script:?}: {e}"),
    }
}

/// Parse and execute a script, returning only the exit status.
pub fn exec_status(script: &str) -> i32 {
    exec_ok(script).1
}

/// Parse and execute a script, returning the exit status or 1 on error.
/// Unlike `exec_ok`, this does not panic on execution errors.
pub fn exec_result(script: &str) -> i32 {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));

    let mut executor = test_executor();

    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => status,
        Err(ExecError::ExitRequested(code)) => code,
        Err(_) => 1,
    }
}

pub fn expect_unsupported(script: &str) {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));
    let mut executor = Executor::new();
    let _ = executor.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    let mut captured = CapturedIo::new();
    let err = executor
        .execute(&program, &mut captured.context())
        .expect_err(&format!("expected UnsupportedFeature for {script:?}"));
    assert!(
        matches!(err, ExecError::UnsupportedFeature(_)),
        "expected UnsupportedFeature, got {err:?} for {script:?}",
    );
}

#[allow(dead_code)]
pub fn expect_unsupported_bash(script: &str) {
    let program =
        thaum::parse_with(script, Dialect::Bash).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));
    let mut executor = Executor::new();
    let mut captured = CapturedIo::new();
    let err = executor
        .execute(&program, &mut captured.context())
        .expect_err(&format!("expected UnsupportedFeature for {script:?}"));
    assert!(
        matches!(err, ExecError::UnsupportedFeature(_)),
        "expected UnsupportedFeature, got {err:?} for {script:?}",
    );
}

/// Parse and execute a Bash-dialect script, returning the exit status or 1 on error.
/// Unlike `bash_exec_ok`, this does not panic on execution errors.
pub fn bash_exec_result(script: &str) -> i32 {
    let program =
        thaum::parse_with(script, Dialect::Bash).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));

    let mut executor = test_bash_executor();

    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => status,
        Err(ExecError::ExitRequested(code)) => code,
        Err(_) => 1,
    }
}

/// Parse and execute a Bash-dialect script, capturing stdout. Returns (stdout, exit_status).
pub fn bash_exec_ok(script: &str) -> (String, i32) {
    let program =
        thaum::parse_with(script, Dialect::Bash).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));

    let mut executor = test_bash_executor();

    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => (captured.stdout_string(), status),
        Err(ExecError::ExitRequested(code)) => (captured.stdout_string(), code),
        Err(e) => panic!("exec failed for {script:?}: {e}"),
    }
}

/// Create an executor whose PATH points only to the test_tools directory.
pub fn test_executor_with_tools(tools: &std::path::Path) -> Executor {
    let mut env = Environment::new();
    let _ = env.set_var("PATH", &tools.to_string_lossy());
    let mut executor = Executor::with_env(env);
    executor.set_exe_path(thaum_exe());
    executor
}

/// Create a Bash-dialect executor whose PATH points only to the test_tools directory.
pub fn test_bash_executor_with_tools(tools: &std::path::Path) -> Executor {
    let mut env = Environment::new();
    let _ = env.set_var("PATH", &tools.to_string_lossy());
    let options = Dialect::Bash.options();
    let mut executor = Executor::with_env_and_options(env, options);
    executor.set_exe_path(thaum_exe());
    executor
}

/// Parse and execute, capturing stdout+stderr, with PATH set to the tools directory.
/// Returns (stdout, stderr, exit_status).
pub fn exec_with_tools(script: &str, tools: &std::path::Path) -> (String, String, i32) {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));
    let mut executor = test_executor_with_tools(tools);
    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => (captured.stdout_string(), captured.stderr_string(), status),
        Err(ExecError::ExitRequested(code)) => (captured.stdout_string(), captured.stderr_string(), code),
        Err(e) => panic!("exec failed for {script:?}: {e}"),
    }
}

/// Parse and execute Bash-dialect, capturing stdout, with PATH set to the tools directory.
pub fn bash_exec_with_tools(script: &str, tools: &std::path::Path) -> (String, String, i32) {
    let program =
        thaum::parse_with(script, Dialect::Bash).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));
    let mut executor = test_bash_executor_with_tools(tools);
    let mut captured = CapturedIo::new();
    match executor.execute(&program, &mut captured.context()) {
        Ok(status) => (captured.stdout_string(), captured.stderr_string(), status),
        Err(ExecError::ExitRequested(code)) => (captured.stdout_string(), captured.stderr_string(), code),
        Err(e) => panic!("exec failed for {script:?}: {e}"),
    }
}

pub fn fixture_dir() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/locale")
        .to_string_lossy()
        .replace('\\', "/")
}

/// Helper: parse and execute with a specific dialect, capturing stdout.
pub fn dialect_exec_ok(script: &str, dialect: Dialect) -> (String, i32) {
    let program = thaum::parse_with(script, dialect)
        .unwrap_or_else(|e| panic!("parse failed for {script:?} with {dialect:?}: {e}"));
    let options = dialect.options();
    let mut exec = Executor::with_options(options);
    exec.set_exe_path(thaum_exe());
    let _ = exec.env_mut().set_var("PATH", "/usr/bin:/bin");
    let mut io = CapturedIo::new();
    match exec.execute(&program, &mut io.context()) {
        Ok(status) => (io.stdout_string(), status),
        Err(ExecError::ExitRequested(code)) => (io.stdout_string(), code),
        Err(e) => panic!("exec failed for {script:?} with {dialect:?}: {e}"),
    }
}
