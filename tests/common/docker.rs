//! Docker sandbox for gauntlet execution tests.
//!
//! Provides two process-scoped fixtures:
//!
//! - **`gauntlet_image`**: builds the Docker image (untagged), removes on drop.
//! - **`gauntlet_sandbox`**: starts a container from the image, kills on drop.
//!
//! The gauntlet test binary is compiled into the Docker image alongside thaum.
//! Exec tests delegate to the binary inside Docker via `docker exec` with
//! `--no-sandbox --format json --exact`.

use std::path::Path;
use std::process::{Command, Stdio};

// Precondition ========================================================================================================

fn docker_available() -> Result<(), String> {
    if thaum_testkit::docker::available() {
        Ok(())
    } else {
        Err("Docker not available".into())
    }
}

// Gauntlet image fixture (process-scoped) =============================================================================

/// A Docker image built from `tests/docker/Dockerfile`. Untagged — identified
/// by raw image ID. Removed on drop (build cache stays).
pub struct GauntletImage {
    pub id: String,
}

impl Drop for GauntletImage {
    fn drop(&mut self) {
        thaum_testkit::docker::remove_image(&self.id);
        eprintln!("gauntlet: removed Docker image {}", &self.id[..12.min(self.id.len())]);
    }
}

#[skuld::fixture(scope = process, requires = [docker_available])]
fn gauntlet_image() -> Result<GauntletImage, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dockerfile = manifest_dir.join("tests/docker/Dockerfile");
    eprintln!("gauntlet: building Docker image...");
    let id = thaum_testkit::docker::build_image(&dockerfile, manifest_dir, None)?;
    eprintln!("gauntlet: built Docker image {}", &id[..12.min(id.len())]);
    Ok(GauntletImage { id })
}

// Gauntlet sandbox fixture (process-scoped) ===========================================================================

/// A running Docker container for gauntlet test execution. Killed on drop.
pub struct GauntletSandbox {
    pub container_id: String,
}

impl Drop for GauntletSandbox {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        eprintln!(
            "gauntlet: removed sandbox container {}",
            &self.container_id[..12.min(self.container_id.len())]
        );
    }
}

#[skuld::fixture(scope = process, requires = [docker_available])]
fn gauntlet_sandbox(#[fixture] gauntlet_image: &GauntletImage) -> Result<GauntletSandbox, String> {
    let output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--network=none",
            "--tmpfs=/tmp:size=64m,exec",
            "--entrypoint",
            "sleep",
            &gauntlet_image.id,
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
        "gauntlet: started sandbox container {}",
        &container_id[..12.min(container_id.len())]
    );
    Ok(GauntletSandbox { container_id })
}

// ExecResult ==========================================================================================================

/// Result from running a test script (used by `run_exec_native` in gauntlet.rs).
#[derive(Debug)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
