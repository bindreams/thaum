//! Docker sandbox for corpus execution tests.
//!
//! Provides a named container fixture (`thaum-corpus-sandbox`).  The container
//! is started on first use and reused across nextest invocations.  Individual
//! tests use `docker exec` against it.

use std::collections::HashMap;
use std::process::{Command, Stdio};

const CONTAINER_NAME: &str = "thaum-corpus-sandbox";
const IMAGE_NAME: &str = "thaum-corpus-exec";

// Container lifecycle =========================================================

/// Check if Docker is available on this host.
pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Check if a Docker image is available locally.
pub fn docker_image_available(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Check if the sandbox container is already running.
fn container_running() -> bool {
    let output = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", CONTAINER_NAME])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok();
    output
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false)
}

/// Ensure the sandbox container is running. Starts it if needed.
///
/// The container runs with `--network=none --tmpfs=/tmp:size=8m,exec` and sleeps
/// forever. Tests use `exec_in_container` to run commands inside it.
pub fn ensure_container() -> bool {
    if container_running() {
        return true;
    }

    // Remove any stopped container with the same name.
    let _ = Command::new("docker")
        .args(["rm", "-f", CONTAINER_NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let status = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            CONTAINER_NAME,
            "--network=none",
            "--tmpfs=/tmp:size=8m,exec",
            "--entrypoint",
            "sleep",
            IMAGE_NAME,
            "infinity",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .ok();

    status.is_some_and(|s| s.success())
}

/// Stop and remove the sandbox container.
pub fn stop_container() {
    let _ = Command::new("docker")
        .args(["rm", "-f", CONTAINER_NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

// Execution ===================================================================

/// Result from running a test script.
#[derive(Debug)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Run a test inside the sandbox Docker container via `docker exec`.
///
/// Creates a per-test temp directory, applies `environment`, runs `setup` (if
/// provided), then executes the test script via `thaum exec`.
pub fn exec_in_container(
    script: &str,
    bash: bool,
    setup: Option<&str>,
    environment: &HashMap<String, String>,
) -> ExecResult {
    let wrapper = build_wrapper(script, bash, setup, environment);

    let output = Command::new("docker")
        .args(["exec", "-i", CONTAINER_NAME, "/bin/sh", "-c", &wrapper])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run docker exec");

    let exit_code = output.status.code().unwrap_or(128);

    ExecResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code,
    }
}

/// Run a test natively on the host (--no-sandbox mode).
pub fn exec_native(script: &str, bash: bool, setup: Option<&str>, environment: &HashMap<String, String>) -> ExecResult {
    let wrapper = build_wrapper(script, bash, setup, environment);

    let output = Command::new("/bin/sh")
        .args(["-c", &wrapper])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run /bin/sh");

    let exit_code = output.status.code().unwrap_or(128);

    ExecResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code,
    }
}

/// Build the shell wrapper that handles environment, setup, and test execution.
fn build_wrapper(script: &str, bash: bool, setup: Option<&str>, environment: &HashMap<String, String>) -> String {
    let bash_flag = if bash { " --bash" } else { "" };

    let mut w = String::new();
    w.push_str("set -e\n");

    // Create a per-test working directory and cd into it.
    w.push_str("workdir=$(mktemp -d)\n");
    w.push_str("cd \"$workdir\"\n");

    // Export environment variables. Double-quoted so $VAR and backticks in
    // values undergo shell expansion — intentional for corpus test flexibility,
    // but assumes trusted test data.
    for (key, value) in environment {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        w.push_str(&format!("export {key}=\"{escaped}\"\n"));
    }

    // Write and execute setup script if provided.
    if let Some(setup_script) = setup {
        w.push_str("cat > .setup <<'__SETUP__'\n");
        w.push_str(setup_script);
        if !setup_script.ends_with('\n') {
            w.push('\n');
        }
        w.push_str("__SETUP__\n");
        w.push_str("chmod +x .setup && ./.setup\n");
    }

    // Add workdir to PATH so helpers created by setup are found as commands.
    w.push_str("export PATH=\"$workdir:$PATH\"\n");

    // Run the test script. `set +e` so thaum's exit code propagates instead
    // of triggering the wrapper's `set -e`.
    w.push_str("set +e\n");
    let escaped_test = script.replace('\'', "'\\''");
    w.push_str(&format!("thaum exec{bash_flag} -c '{escaped_test}'\n"));

    w
}
