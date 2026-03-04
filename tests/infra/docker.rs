//! Docker infrastructure smoke tests.
//!
//! Verify that Docker images build and containers work, independently of
//! whether corpus or bench tests are enabled. Catches Dockerfile breakage
//! early.

use std::path::Path;
use std::process::{Command, Stdio};

use super::preconditions;

skuld::default_labels!(infra, docker);

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

// Fixtures ====================================================================

/// A built Docker image, removed on drop.
pub struct DockerImage {
    pub id: String,
}

impl Drop for DockerImage {
    fn drop(&mut self) {
        thaum::testkit::docker::remove_image(&self.id);
    }
}

#[skuld::fixture(scope = process, name = "infra_corpus_image", requires = [preconditions::docker])]
fn corpus_image() -> Result<DockerImage, String> {
    let dockerfile = project_root().join("tests/docker/Dockerfile");
    thaum::testkit::docker::build_image(&dockerfile, project_root(), None).map(|id| DockerImage { id })
}

#[skuld::fixture(scope = process, name = "infra_bench_image", requires = [preconditions::docker])]
fn bench_image() -> Result<DockerImage, String> {
    let dockerfile = project_root().join("benches/docker/Dockerfile");
    thaum::testkit::docker::build_image(&dockerfile, project_root(), None).map(|id| DockerImage { id })
}

// Helpers =====================================================================

/// Start a detached container from an image, returning the container ID.
fn start_container(image_id: &str) -> String {
    let output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "--network=none",
            "--entrypoint",
            "sleep",
            image_id,
            "60",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run docker");
    assert!(
        output.status.success(),
        "docker run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Execute a command inside a container and return (stdout, exit_code).
fn exec_in(container_id: &str, cmd: &[&str]) -> (String, i32) {
    let mut args = vec!["exec", container_id];
    args.extend_from_slice(cmd);
    let output = Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run docker exec");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let code = output.status.code().unwrap_or(128);
    (stdout, code)
}

/// Kill and remove a container.
fn kill_container(container_id: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", container_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

// Tests -----------------------------------------------------------------------

#[skuld::test]
fn corpus_image_builds(#[fixture(infra_corpus_image)] image: &DockerImage) {
    assert!(!image.id.is_empty(), "corpus image should have an ID");
}

#[skuld::test]
fn bench_image_builds(#[fixture(infra_bench_image)] image: &DockerImage) {
    assert!(!image.id.is_empty(), "bench image should have an ID");
}

#[skuld::test]
fn corpus_container_lifecycle(#[fixture(infra_corpus_image)] image: &DockerImage) {
    let container_id = start_container(&image.id);
    let (stdout, code) = exec_in(&container_id, &["echo", "ok"]);
    kill_container(&container_id);

    assert_eq!(code, 0, "echo should exit 0");
    assert_eq!(stdout.trim(), "ok", "echo should output 'ok'");
}

#[skuld::test]
fn corpus_thaum_available(#[fixture(infra_corpus_image)] image: &DockerImage) {
    let container_id = start_container(&image.id);
    let (stdout, code) = exec_in(&container_id, &["thaum", "--version"]);
    kill_container(&container_id);

    assert_eq!(code, 0, "thaum --version should exit 0");
    assert!(
        stdout.contains("thaum"),
        "thaum --version should mention 'thaum', got: {stdout:?}"
    );
}
