//! Shared helpers for conformance tests.

use std::io::Write;
use std::process::{Command, Stdio};

use thaum::exec::{CapturedIo, ExecError, Executor};

#[derive(Debug)]
#[allow(dead_code)]
pub struct ShellResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Run a script in our executor, capturing stdout.
pub fn run_ours(script: &str) -> ShellResult {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    let mut executor = Executor::new();
    let _ = executor.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");

    let mut captured = CapturedIo::new();
    let exit_code = match executor.execute(&program, &mut captured.context()) {
        Ok(status) => status,
        Err(ExecError::ExitRequested(code)) => code,
        Err(e) => panic!("exec failed for {:?}: {}", script, e),
    };

    ShellResult {
        stdout: captured.stdout_string(),
        stderr: captured.stderr_string(),
        exit_code,
    }
}

/// Run a script in a Docker container with the given shell.
pub fn run_in_docker(script: &str, shell: &str) -> ShellResult {
    let mut child = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-i",
            "--network=none",
            "--read-only",
            "--tmpfs=/tmp:size=1m",
            "shell-exec-test",
            shell,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start docker");

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(script.as_bytes()).expect("write to docker stdin");
    }
    child.stdin.take();

    let output = child.wait_with_output().expect("docker wait");

    if !output.status.success() && output.stdout.is_empty() {
        panic!(
            "Docker runner failed for shell={}, script={:?}\nstderr: {}",
            shell,
            script,
            String::from_utf8_lossy(&output.stderr)
        );
    }

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

    ShellResult {
        stdout: decode_base64(&stdout_b64),
        stderr: decode_base64(&stderr_b64),
        exit_code,
    }
}

fn decode_base64(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
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

/// Assert our executor produces the same exit code as both reference shells.
pub fn assert_exit_matches_both(script: &str) {
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
pub fn assert_shells_agree(script: &str) {
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
