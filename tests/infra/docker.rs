//! Docker infrastructure smoke tests.
//!
//! Verify that Docker images build and containers work, independently of
//! whether corpus or bench tests are enabled. Catches Dockerfile breakage
//! early.

use std::path::Path;
use std::process::{Command, Stdio};

use super::preconditions;

testutil::default_labels!(infra, docker);

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Build an untagged Docker image and return its ID. Caller must remove it.
fn build_image(dockerfile: &Path) -> String {
    testutil::docker::build_image(dockerfile, project_root(), None)
        .unwrap_or_else(|e| panic!("Docker build failed for {}: {e}", dockerfile.display()))
}

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
    assert!(output.status.success(), "docker run failed");
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

#[testutil::test(requires = [preconditions::docker])]
fn corpus_image_builds() {
    let dockerfile = project_root().join("tests/docker/Dockerfile");
    let id = build_image(&dockerfile);
    testutil::docker::remove_image(&id);
}

#[testutil::test(requires = [preconditions::docker])]
fn bench_image_builds() {
    let dockerfile = project_root().join("benches/docker/Dockerfile");
    let id = build_image(&dockerfile);
    testutil::docker::remove_image(&id);
}

#[testutil::test(requires = [preconditions::docker])]
fn corpus_container_lifecycle() {
    let dockerfile = project_root().join("tests/docker/Dockerfile");
    let image_id = build_image(&dockerfile);

    let container_id = start_container(&image_id);
    let (stdout, code) = exec_in(&container_id, &["echo", "ok"]);

    kill_container(&container_id);
    testutil::docker::remove_image(&image_id);

    assert_eq!(code, 0, "echo should exit 0");
    assert_eq!(stdout.trim(), "ok", "echo should output 'ok'");
}

#[testutil::test(requires = [preconditions::docker])]
fn corpus_thaum_available() {
    let dockerfile = project_root().join("tests/docker/Dockerfile");
    let image_id = build_image(&dockerfile);

    let container_id = start_container(&image_id);
    let (stdout, code) = exec_in(&container_id, &["thaum", "--version"]);

    kill_container(&container_id);
    testutil::docker::remove_image(&image_id);

    assert_eq!(code, 0, "thaum --version should exit 0");
    assert!(
        stdout.contains("thaum"),
        "thaum --version should mention 'thaum', got: {stdout:?}"
    );
}
