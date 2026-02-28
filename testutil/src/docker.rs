//! Docker helpers for test infrastructure.

use std::path::Path;
use std::process::{Command, Stdio};

/// Check if the Docker daemon is running.
pub fn available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Check if a Docker image exists locally.
pub fn image_available(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Build a Docker image. Returns the image ID.
///
/// If `tag` is provided, the image is tagged (e.g. `thaum-corpus-exec`).
/// If `None`, the untagged image ID is returned (for one-shot use).
pub fn build_image(dockerfile: &Path, context: &Path, tag: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new("docker");
    cmd.arg("build").arg("--quiet");

    if let Some(tag) = tag {
        cmd.args(["-t", tag]);
    }

    cmd.args(["-f", &dockerfile.to_string_lossy()]).arg(context);

    let output = cmd.output().map_err(|e| format!("docker build failed to start: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker build failed: {stderr}"));
    }

    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if let Some(tag) = tag {
        Ok(tag.to_string())
    } else {
        Ok(id)
    }
}

/// Remove a Docker image by ID or tag.
pub fn remove_image(image: &str) {
    let _ = Command::new("docker")
        .args(["rmi", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
