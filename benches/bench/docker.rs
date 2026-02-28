//! Docker build and run helpers.

use std::path::Path;
use std::process::Command;

pub fn available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub fn build_image() -> Option<String> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dockerfile = project_root.join("benches/docker/bench.Dockerfile");

    eprintln!("Building Docker image...");
    let output = Command::new("docker")
        .arg("build")
        .arg("--quiet")
        .args(["-f", &dockerfile.to_string_lossy()])
        .arg(project_root)
        .output()
        .ok()?;

    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run the bench binary inside Docker with a read-only volume mount.
/// Returns stdout bytes.
pub fn run_with_volume(image_id: &str, volume: &str, args: &[&str]) -> Option<Vec<u8>> {
    let mut cmd_args = vec!["run", "--rm", "-v", volume, image_id, "bench"];
    cmd_args.extend_from_slice(args);

    let output = Command::new("docker").args(&cmd_args).output().ok()?;

    // Remove the untagged image; build cache layers stay.
    let _ = Command::new("docker")
        .args(["rmi", image_id])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    Some(output.stdout)
}
