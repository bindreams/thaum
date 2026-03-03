//! Docker sandbox for corpus execution tests.
//!
//! Provides two process-scoped fixtures:
//!
//! - **`corpus_image`**: builds the thaum Docker image (untagged), removes on drop.
//! - **`corpus_sandbox`**: starts a container from the image, kills on drop.
//!
//! Tests use [`exec_in_container`] to run scripts inside the sandbox.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

// Precondition ================================================================

fn docker_available() -> Result<(), String> {
    if testutil::docker::available() {
        Ok(())
    } else {
        Err("Docker not available".into())
    }
}

// Corpus image fixture (process-scoped) =======================================

/// A Docker image built from `tests/docker/Dockerfile`. Untagged — identified
/// by raw image ID. Removed on drop (build cache stays).
pub struct CorpusImage {
    pub id: String,
}

impl Drop for CorpusImage {
    fn drop(&mut self) {
        testutil::docker::remove_image(&self.id);
        eprintln!("corpus: removed Docker image {}", &self.id[..12.min(self.id.len())]);
    }
}

#[testutil::fixture(scope = process, requires = [docker_available])]
fn corpus_image() -> Result<CorpusImage, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dockerfile = manifest_dir.join("tests/docker/Dockerfile");
    eprintln!("corpus: building Docker image...");
    let id = testutil::docker::build_image(&dockerfile, manifest_dir, None)?;
    eprintln!("corpus: built Docker image {}", &id[..12.min(id.len())]);
    Ok(CorpusImage { id })
}

// Corpus sandbox fixture (process-scoped) =====================================

/// A running Docker container for corpus test execution. Killed on drop.
pub struct CorpusSandbox {
    pub container_id: String,
}

impl Drop for CorpusSandbox {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        eprintln!(
            "corpus: removed sandbox container {}",
            &self.container_id[..12.min(self.container_id.len())]
        );
    }
}

#[testutil::fixture(scope = process, requires = [docker_available])]
fn corpus_sandbox(#[fixture] corpus_image: &CorpusImage) -> Result<CorpusSandbox, String> {
    let output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--network=none",
            "--tmpfs=/tmp:size=64m,exec",
            "--entrypoint",
            "sleep",
            &corpus_image.id,
            "infinity",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to start container: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker run failed: {stderr}"));
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    eprintln!(
        "corpus: started sandbox container {}",
        &container_id[..12.min(container_id.len())]
    );
    Ok(CorpusSandbox { container_id })
}

// Execution ===================================================================

/// Result from running a test script.
#[derive(Debug)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Run a test inside a Docker container via `docker exec`.
///
/// Creates a per-test temp directory, applies `environment`, runs `setup` (if
/// provided), then executes the test script via `thaum exec`.
pub fn exec_in_container(
    container_id: &str,
    script: &str,
    bash: bool,
    setup: Option<&str>,
    environment: &HashMap<String, String>,
) -> ExecResult {
    let wrapper = build_wrapper(script, bash, setup, environment);

    let output = Command::new("docker")
        .args(["exec", "-i", container_id, "/bin/sh", "-c", &wrapper])
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

    // Export environment variables.
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
