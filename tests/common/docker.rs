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
