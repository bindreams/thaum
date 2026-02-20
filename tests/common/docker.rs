use std::io::Write;
use std::process::{Command, Stdio};

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
        "run", "--rm", "-i",
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
            "run", "--rm", "-i",
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

    // Parse the base64-encoded output format from conformance_runner.sh
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

    DockerResult {
        stdout: decode_base64(&stdout_b64),
        stderr: decode_base64(&stderr_b64),
        exit_code,
    }
}

/// Decode base64 using the system `base64` command.
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
