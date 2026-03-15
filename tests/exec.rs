//! Execution tests: verify runtime behavior of the thaum executor.

pub use skuld::temp_dir;
pub use thaum_test_tools::test_tools;

#[path = "common/mod.rs"]
mod common;

use std::cell::Cell;

use thaum::exec::{CapturedIo, Environment, ExecError, Executor};
use thaum::Dialect;

fn main() {
    skuld::run_all();
}

skuld::default_labels!(lex, parse, exec);

const DEFAULT_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

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

pub fn fixture_dir() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/locale")
        .to_string_lossy()
        .replace('\\', "/")
}

// ExecOutput ==========================================================================================================

/// Result of executing a shell script in a test.
///
/// All three fields (stdout, stderr, status) are tracked. On drop, unchecked
/// fields trigger assertions: stdout must be empty, stderr must be empty,
/// status must be 0. This ensures tests never silently swallow output or errors.
pub struct ExecOutput {
    stdout: String,
    stderr: String,
    status: i32,
    stdout_checked: Cell<bool>,
    stderr_checked: Cell<bool>,
    status_checked: Cell<bool>,
}

impl ExecOutput {
    fn new(stdout: String, stderr: String, status: i32) -> Self {
        Self {
            stdout,
            stderr,
            status,
            stdout_checked: Cell::new(false),
            stderr_checked: Cell::new(false),
            status_checked: Cell::new(false),
        }
    }

    pub fn stdout(&self) -> &str {
        self.stdout_checked.set(true);
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        self.stderr_checked.set(true);
        &self.stderr
    }

    pub fn status(&self) -> i32 {
        self.status_checked.set(true);
        self.status
    }
}

impl Drop for ExecOutput {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        if !self.stdout_checked.get() {
            assert!(
                self.stdout.is_empty(),
                "unchecked stdout was non-empty:\n{}",
                self.stdout
            );
        }
        if !self.status_checked.get() {
            assert_eq!(self.status, 0, "unchecked exit status was {}", self.status);
        }
        if !self.stderr_checked.get() {
            assert!(
                self.stderr.is_empty(),
                "unchecked stderr was non-empty:\n{}",
                self.stderr
            );
        }
    }
}

// ExecMode ============================================================================================================

/// Execution mode for `exec!` and `shell!`.
#[derive(Clone, Copy)]
pub enum ExecMode {
    /// Run in-process via `Executor`. Process replacement is disallowed.
    InProcess,
    /// Spawn a `thaum` subprocess. Natural process replacement.
    Subprocess,
}

// exec! macro + builder ===============================================================================================

/// One-shot script execution builder. Use the `exec!` macro for ergonomic access.
pub struct ExecBuilder<'a> {
    script: &'a str,
    dialect: Dialect,
    mode: ExecMode,
    env: Vec<(&'a str, &'a str)>,
}

impl<'a> ExecBuilder<'a> {
    pub fn new(script: &'a str) -> Self {
        Self {
            script,
            dialect: Dialect::Posix,
            mode: ExecMode::InProcess,
            env: Vec::new(),
        }
    }

    pub fn dialect(mut self, d: Dialect) -> Self {
        self.dialect = d;
        self
    }

    pub fn mode(mut self, m: ExecMode) -> Self {
        self.mode = m;
        self
    }

    pub fn env(mut self, vars: &[(&'a str, &'a str)]) -> Self {
        self.env.extend_from_slice(vars);
        self
    }

    pub fn run(self) -> ExecOutput {
        match self.mode {
            ExecMode::InProcess => run_in_process(self.script, self.dialect, &self.env),
            ExecMode::Subprocess => run_subprocess(self.script, self.dialect, &self.env),
        }
    }
}

/// Execute a shell script and return an `ExecOutput` with drop-assertions.
///
/// ```ignore
/// exec!("echo hello")
/// exec!("echo hello", dialect = Dialect::Bash)
/// exec!("echo hello", env = &[("PATH", &tools_dir)])
/// exec!("cmd", mode = ExecMode::Subprocess, env = &[("PATH", &dir)])
/// ```
macro_rules! exec {
    ($script:expr $(, $($rest:tt)*)?) => {
        exec!(@build crate::ExecBuilder::new($script) $(, $($rest)*)?)
    };
    (@build $builder:expr $(,)?) => { $builder.run() };
    (@build $builder:expr, dialect = $val:expr $(, $($rest:tt)*)?) => {
        exec!(@build $builder.dialect($val) $(, $($rest)*)?)
    };
    (@build $builder:expr, mode = $val:expr $(, $($rest:tt)*)?) => {
        exec!(@build $builder.mode($val) $(, $($rest)*)?)
    };
    (@build $builder:expr, env = $val:expr $(, $($rest:tt)*)?) => {
        exec!(@build $builder.env($val) $(, $($rest)*)?)
    };
}

// shell! macro + Shell ================================================================================================

/// Persistent shell session builder. Use the `shell!` macro for ergonomic access.
pub struct ShellBuilder<'a> {
    dialect: Dialect,
    interactive: bool,
    env: Vec<(&'a str, &'a str)>,
}

impl Default for ShellBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ShellBuilder<'a> {
    pub fn new() -> Self {
        Self {
            dialect: Dialect::Posix,
            interactive: false,
            env: Vec::new(),
        }
    }

    pub fn dialect(mut self, d: Dialect) -> Self {
        self.dialect = d;
        self
    }

    pub fn interactive(mut self) -> Self {
        self.interactive = true;
        self
    }

    pub fn env(mut self, vars: &[(&'a str, &'a str)]) -> Self {
        self.env.extend_from_slice(vars);
        self
    }

    pub fn build(self) -> Shell {
        let options = self.dialect.options();
        let mut env = Environment::new();
        let _ = env.set_var("PATH", DEFAULT_PATH);
        for (k, v) in &self.env {
            let _ = env.set_var(k, v);
        }
        let mut executor = Executor::with_env_and_options(env, options);
        executor.set_exe_path(thaum_exe());
        executor.set_allow_process_replacement(false);
        if self.interactive {
            executor.env_mut().set_interactive(true);
        }
        Shell {
            executor,
            dialect: self.dialect,
            last_status: 0,
            accumulated_stderr: String::new(),
            joined: false,
        }
    }
}

/// Persistent shell session for multi-step execution tests.
///
/// Must be consumed via `join()`. Dropping without `join()` triggers a panic
/// (unless already panicking).
pub struct Shell {
    executor: Executor,
    dialect: Dialect,
    last_status: i32,
    accumulated_stderr: String,
    joined: bool,
}

impl Shell {
    /// Execute a script in this shell session.
    pub fn exec(&mut self, script: &str) -> ExecOutput {
        // TODO: when alias-aware parsing is available, snapshot the alias table
        // before parsing in interactive mode (matching real REPL behavior).
        let program =
            thaum::parse_with(script, self.dialect).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));

        let mut captured = CapturedIo::new();
        let status = match self.executor.execute(&program, &mut captured.context()) {
            Ok(s) => s,
            Err(ExecError::ExitRequested(code)) => code,
            Err(e) => panic!("unexpected error leaked past executor for {script:?}: {e}"),
        };
        self.last_status = status;
        let stderr = captured.stderr_string();
        self.accumulated_stderr.push_str(&stderr);
        ExecOutput::new(captured.stdout_string(), stderr, status)
    }

    /// Write data to a file descriptor's buffer (stdin = 0).
    pub fn write(&mut self, _fd: i32, _data: &[u8]) {
        // TODO: implement stdin injection via CapturedIo::with_stdin
        // For now, this is a placeholder. The current architecture requires
        // stdin to be set up before execution, not mid-session.
        unimplemented!("Shell::write() not yet implemented");
    }

    /// Access the live environment.
    pub fn env(&self) -> &Environment {
        self.executor.env()
    }

    /// Access the live environment mutably.
    pub fn env_mut(&mut self) -> &mut Environment {
        self.executor.env_mut()
    }

    /// Consume the shell and return a final `ExecOutput`.
    ///
    /// The returned output has: empty stdout, accumulated stderr, last status.
    pub fn join(mut self) -> ExecOutput {
        self.joined = true;
        ExecOutput::new(
            String::new(),
            std::mem::take(&mut self.accumulated_stderr),
            self.last_status,
        )
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        if !self.joined {
            panic!("Shell dropped without join() — call sh.join() to check final state");
        }
    }
}

/// Create a persistent shell session.
///
/// ```ignore
/// let mut sh = shell!();
/// let mut sh = shell!(dialect = Dialect::Bash);
/// let mut sh = shell!(dialect = Dialect::Bash, interactive);
/// let mut sh = shell!(env = &[("PATH", &tools_dir)]);
/// ```
macro_rules! shell {
    () => { crate::ShellBuilder::new().build() };
    (dialect = $($rest:tt)*) => {
        shell!(@build crate::ShellBuilder::new(), dialect = $($rest)*)
    };
    (interactive $($rest:tt)*) => {
        shell!(@build crate::ShellBuilder::new(), interactive $($rest)*)
    };
    (env = $($rest:tt)*) => {
        shell!(@build crate::ShellBuilder::new(), env = $($rest)*)
    };
    (@build $builder:expr $(,)?) => { $builder.build() };
    (@build $builder:expr, dialect = $val:expr $(, $($rest:tt)*)?) => {
        shell!(@build $builder.dialect($val) $(, $($rest)*)?)
    };
    (@build $builder:expr, interactive $(, $($rest:tt)*)?) => {
        shell!(@build $builder.interactive() $(, $($rest)*)?)
    };
    (@build $builder:expr, env = $val:expr $(, $($rest:tt)*)?) => {
        shell!(@build $builder.env($val) $(, $($rest)*)?)
    };
}

// Test modules ========================================================================================================

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

// Core execution functions ============================================================================================

fn run_in_process(script: &str, dialect: Dialect, extra_env: &[(&str, &str)]) -> ExecOutput {
    let program = thaum::parse_with(script, dialect).unwrap_or_else(|e| panic!("parse failed for {script:?}: {e}"));

    let options = dialect.options();
    let mut env = Environment::new();
    let _ = env.set_var("PATH", DEFAULT_PATH);
    for (k, v) in extra_env {
        let _ = env.set_var(k, v);
    }
    let mut executor = Executor::with_env_and_options(env, options);
    executor.set_exe_path(thaum_exe());
    executor.set_allow_process_replacement(false);

    let mut captured = CapturedIo::new();
    let status = match executor.execute(&program, &mut captured.context()) {
        Ok(s) => s,
        Err(ExecError::ExitRequested(code)) => code,
        Err(e) => panic!("unexpected error leaked past executor for {script:?}: {e}"),
    };
    ExecOutput::new(captured.stdout_string(), captured.stderr_string(), status)
}

fn run_subprocess(script: &str, dialect: Dialect, extra_env: &[(&str, &str)]) -> ExecOutput {
    let mut cmd = std::process::Command::new(thaum_exe());
    cmd.args(["--dialect", &dialect.to_string(), "exec", "-c", script]);
    cmd.env_clear();
    cmd.env("PATH", DEFAULT_PATH);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let output = cmd.output().unwrap_or_else(|e| panic!("failed to spawn thaum: {e}"));
    ExecOutput::new(
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(128),
    )
}

// Raw executor helpers (for tests that need direct Executor access) ===================================================

/// Create an executor configured for tests (controlled PATH, thaum exe path).
///
/// Only for tests that need raw executor access (stdin injection, IoContext testing).
/// For normal tests, use `exec!` or `shell!` instead.
pub fn test_executor() -> Executor {
    let mut env = Environment::new();
    let _ = env.set_var("PATH", DEFAULT_PATH);
    let mut executor = Executor::with_env(env);
    executor.set_exe_path(thaum_exe());
    executor
}

/// Create an executor with tools directory on PATH.
///
/// Only for tests that need raw executor access (IoContext testing).
pub fn test_executor_with_tools(tools: &std::path::Path) -> Executor {
    let mut env = Environment::new();
    let _ = env.set_var("PATH", &tools.to_string_lossy());
    let mut executor = Executor::with_env(env);
    executor.set_exe_path(thaum_exe());
    executor
}
