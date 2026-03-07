//! Docker sandbox for corpus execution tests.
//!
//! Provides two process-scoped fixtures:
//!
//! - **`corpus_image`**: builds the Docker image (untagged), removes on drop.
//! - **`corpus_sandbox`**: starts a container from the image, kills on drop.
//!
//! The corpus test binary is compiled into the Docker image alongside thaum.
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

// Corpus image fixture (process-scoped) ===============================================================================

/// A Docker image built from `tests/docker/Dockerfile`. Untagged — identified
/// by raw image ID. Removed on drop (build cache stays).
pub struct CorpusImage {
    pub id: String,
}

impl Drop for CorpusImage {
    fn drop(&mut self) {
        thaum_testkit::docker::remove_image(&self.id);
        eprintln!("corpus: removed Docker image {}", &self.id[..12.min(self.id.len())]);
    }
}

#[skuld::fixture(scope = process, requires = [docker_available])]
fn corpus_image() -> Result<CorpusImage, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dockerfile = manifest_dir.join("tests/docker/Dockerfile");
    eprintln!("corpus: building Docker image...");
    let id = thaum_testkit::docker::build_image(&dockerfile, manifest_dir, None)?;
    eprintln!("corpus: built Docker image {}", &id[..12.min(id.len())]);
    Ok(CorpusImage { id })
}

// Corpus sandbox fixture (process-scoped) =============================================================================

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

#[skuld::fixture(scope = process, requires = [docker_available])]
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

// ExecResult ==========================================================================================================

/// Result from running a test script (used by `run_exec_native` in corpus.rs).
#[derive(Debug)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
