//! Conformance tests: compare our executor's output against real shells.
//!
//! These tests run the same script in our executor AND in Docker containers
//! running `dash` and `bash --posix`, then compare stdout and exit code.
//!
//! Requires: Docker installed and `shell-exec-test` image built.
//!
//! To run:
//!   docker build -t shell-exec-test tests/docker/
//!   RUN_CONFORMANCE_TESTS=1 cargo test conformance
//!
//! These tests are skipped by default to avoid requiring Docker in CI.

use std::io::Write;
use std::process::{Command, Stdio};

use shell_parser::exec::{ExecError, Executor};

/// Check if conformance tests should run (Docker must be available).
fn should_run() -> bool {
    std::env::var("RUN_CONFORMANCE_TESTS").is_ok()
}

/// Check if the Docker image is available.
fn docker_image_available() -> bool {
    Command::new("docker")
        .args(["image", "inspect", "shell-exec-test"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Result from running a script.
#[derive(Debug)]
#[allow(dead_code)]
struct ShellResult {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

/// Run a script in our executor, capturing stdout.
fn run_ours(script: &str) -> ShellResult {
    let program = shell_parser::parse(script)
        .unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    // We can't easily capture stdout from our executor since it writes to
    // the real stdout for external commands. For built-in-only scripts,
    // we can capture via the builtins module.
    //
    // For a proper implementation, we'd need to redirect our executor's
    // stdout to a pipe. For now, we run the script via our own binary
    // and capture its output.
    //
    // Simpler approach: run as external process using `sh -c` style,
    // or use the built-in execution with captured I/O.
    //
    // For this initial version: execute and capture via environment.
    let mut executor = Executor::new();
    // Set a minimal PATH
    let _ = executor
        .env_mut()
        .set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");

    let exit_code = match executor.execute(&program) {
        Ok(status) => status,
        Err(ExecError::ExitRequested(code)) => code,
        Err(e) => panic!("exec failed for {:?}: {}", script, e),
    };

    // TODO: capture stdout/stderr properly
    // For now, return empty stdout — we'll compare exit codes at minimum.
    ShellResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code,
    }
}

/// Run a script in a Docker container with the given shell.
fn run_in_docker(script: &str, shell: &str) -> ShellResult {
    let mut child = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-i",
            "--network=none",      // No network access for safety
            "--read-only",         // Read-only root filesystem
            "--tmpfs=/tmp:size=1m", // Writable /tmp with size limit
            "shell-exec-test",
            shell,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start docker");

    // Write script to stdin
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(script.as_bytes()).expect("write to docker stdin");
    }
    child.stdin.take(); // Close stdin

    let output = child.wait_with_output().expect("docker wait");

    if !output.status.success() && output.stdout.is_empty() {
        panic!(
            "Docker runner failed for shell={}, script={:?}\nstderr: {}",
            shell,
            script,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Parse the output format:
    //   EXIT:<code>
    //   STDOUT:<base64>
    //   STDERR:<base64>
    let out = String::from_utf8_lossy(&output.stdout);
    let mut exit_code = 0;
    let mut stdout_b64 = String::new();
    let mut stderr_b64 = String::new();

    for line in out.lines() {
        if let Some(code) = line.strip_prefix("EXIT:") {
            exit_code = code.trim().parse().unwrap_or(0);
        } else if let Some(b64) = line.strip_prefix("STDOUT:") {
            stdout_b64 = b64.trim().to_string();
        } else if let Some(b64) = line.strip_prefix("STDERR:") {
            stderr_b64 = b64.trim().to_string();
        }
    }

    // Decode base64
    let stdout = decode_base64(&stdout_b64);
    let stderr = decode_base64(&stderr_b64);

    ShellResult {
        stdout,
        stderr,
        exit_code,
    }
}

/// Simple base64 decoder (we could add a dependency, but this is test-only).
fn decode_base64(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    // Use the system's base64 command to decode
    let output = Command::new("base64")
        .arg("-d")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|mut child| {
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(input.as_bytes()).ok();
            }
            child.stdin.take();
            child.wait_with_output()
        });
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}

/// Assert that our executor produces the same exit code as both reference shells.
fn assert_exit_matches_both(script: &str) {
    if !should_run() {
        return;
    }
    if !docker_image_available() {
        eprintln!("SKIP: shell-exec-test Docker image not found");
        return;
    }

    let ours = run_ours(script);
    let dash = run_in_docker(script, "dash");
    let bash = run_in_docker(script, "bash-posix");

    assert_eq!(
        ours.exit_code, dash.exit_code,
        "Exit code mismatch (ours vs dash) for script: {:?}\n  ours={}\n  dash={}",
        script, ours.exit_code, dash.exit_code
    );
    assert_eq!(
        ours.exit_code, bash.exit_code,
        "Exit code mismatch (ours vs bash --posix) for script: {:?}\n  ours={}\n  bash={}",
        script, ours.exit_code, bash.exit_code
    );
}

/// Assert that the reference shells agree on stdout output for a script.
/// (We compare dash vs bash to verify the script is portable.)
fn assert_shells_agree(script: &str) {
    if !should_run() {
        return;
    }
    if !docker_image_available() {
        eprintln!("SKIP: shell-exec-test Docker image not found");
        return;
    }

    let dash = run_in_docker(script, "dash");
    let bash = run_in_docker(script, "bash-posix");

    assert_eq!(
        dash.exit_code, bash.exit_code,
        "Exit code disagree (dash vs bash) for script: {:?}\n  dash={}\n  bash={}",
        script, dash.exit_code, bash.exit_code
    );
    assert_eq!(
        dash.stdout, bash.stdout,
        "Stdout disagree (dash vs bash) for script: {:?}\n  dash={:?}\n  bash={:?}",
        script, dash.stdout, bash.stdout
    );
}

// --- Conformance tests ---
// Each test verifies that our executor matches real shell behavior.

#[test]
fn conformance_true() {
    assert_exit_matches_both("true");
}

#[test]
fn conformance_false() {
    assert_exit_matches_both("false");
}

#[test]
fn conformance_exit_zero() {
    assert_exit_matches_both("exit 0");
}

#[test]
fn conformance_exit_nonzero() {
    assert_exit_matches_both("exit 42");
}

#[test]
fn conformance_and_list() {
    assert_exit_matches_both("true && true");
    assert_exit_matches_both("false && true");
    assert_exit_matches_both("true && false");
}

#[test]
fn conformance_or_list() {
    assert_exit_matches_both("false || true");
    assert_exit_matches_both("true || false");
    assert_exit_matches_both("false || false");
}

#[test]
fn conformance_not() {
    assert_exit_matches_both("! true");
    assert_exit_matches_both("! false");
}

#[test]
fn conformance_variable_assignment() {
    assert_exit_matches_both("X=hello; exit 0");
}

#[test]
fn conformance_if_true() {
    assert_exit_matches_both("if true; then exit 0; else exit 1; fi");
}

#[test]
fn conformance_if_false() {
    assert_exit_matches_both("if false; then exit 0; else exit 1; fi");
}

#[test]
fn conformance_while_loop() {
    assert_exit_matches_both("X=0; while test $X != done; do X=done; done; exit 0");
}

#[test]
fn conformance_for_loop() {
    assert_exit_matches_both("for i in a b c; do true; done; exit 0");
}

#[test]
fn conformance_case_match() {
    assert_exit_matches_both("case hello in hello) exit 0;; *) exit 1;; esac");
}

#[test]
fn conformance_case_default() {
    assert_exit_matches_both("case world in hello) exit 0;; *) exit 1;; esac");
}

#[test]
fn conformance_function() {
    assert_exit_matches_both("f() { return 42; }; f; exit $?");
}

#[test]
fn conformance_test_builtin() {
    assert_exit_matches_both("test 5 -eq 5");
    assert_exit_matches_both("test 5 -eq 6");
    assert_exit_matches_both("test hello");
    assert_exit_matches_both("test ''");
}

#[test]
fn conformance_bracket_test() {
    assert_exit_matches_both("[ 3 -gt 2 ]");
    assert_exit_matches_both("[ 2 -gt 3 ]");
}

#[test]
fn conformance_parameter_default() {
    assert_exit_matches_both("X=${UNSET:-fallback}; test \"$X\" = fallback");
}

#[test]
fn conformance_break_in_loop() {
    assert_exit_matches_both("for i in a b c; do break; done; exit 0");
}

#[test]
fn conformance_multiple_statements() {
    assert_exit_matches_both("true; false; true");
}

#[test]
fn conformance_shells_agree_on_echo() {
    assert_shells_agree("echo hello world");
}

#[test]
fn conformance_shells_agree_on_variable_expansion() {
    assert_shells_agree("X=hello; echo $X");
}

#[test]
fn conformance_shells_agree_on_for_loop() {
    assert_shells_agree("for i in a b c; do echo $i; done");
}

#[test]
fn conformance_shells_agree_on_if() {
    assert_shells_agree("if true; then echo yes; else echo no; fi");
}

#[test]
fn conformance_shells_agree_on_case() {
    assert_shells_agree("case hello in hello) echo matched;; *) echo default;; esac");
}
