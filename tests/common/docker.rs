use std::io::Write;
use std::process::{Command, Stdio};

use base64::Engine;

/// Result from running a script in Docker.
#[derive(Debug)]
pub struct DockerResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Check if a Docker image is available locally.
pub fn docker_image_available(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a script through `thaum exec` inside the `thaum-corpus-exec` Docker image.
///
/// The Docker container runs with `--network=none --read-only --tmpfs=/tmp:size=1m`
/// for safety. The script is passed via `-c`.
pub fn run_thaum_in_docker(script: &str, bash: bool) -> DockerResult {
    let mut args = vec![
        "run",
        "--rm",
        "-i",
        "--network=none",
        "--read-only",
        "--tmpfs=/tmp:size=1m",
        "thaum-corpus-exec",
    ];
    if bash {
        args.push("--bash");
    }
    args.push("-c");
    args.push(script);

    let output = Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run docker");

    let exit_code = output.status.code().unwrap_or(128);

    DockerResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code,
    }
}

/// Run a script in a Docker container with a reference shell (dash or bash).
/// Uses the `shell-exec-test` image and the base64-encoding conformance runner.
pub fn run_in_reference_shell(script: &str, shell: &str) -> DockerResult {
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
            "Docker runner failed for shell={shell}, script={script:?}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    parse_conformance_runner_output(&output.stdout)
}

/// Parse the base64-encoded output format produced by `conformance_runner.sh`.
///
/// The runner outputs lines like:
/// ```text
/// EXIT:<code>
/// STDOUT:<base64>
/// STDERR:<base64>
/// ```
fn parse_conformance_runner_output(raw: &[u8]) -> DockerResult {
    let out = String::from_utf8_lossy(raw);
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

    DockerResult {
        stdout: decode_base64(&stdout_b64),
        stderr: decode_base64(&stderr_b64),
        exit_code,
    }
}

/// Decode a base64 string, panicking on invalid input.
fn decode_base64(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .unwrap_or_else(|e| panic!("invalid base64 in conformance output: {e}"));
    String::from_utf8(bytes).unwrap_or_else(|e| panic!("non-UTF-8 in conformance output: {e}"))
}
