//! Preconditions for conformance tests.

pub fn docker_conformance_image() -> Result<(), String> {
    if !testutil::docker::available() {
        return Err("Docker not running".into());
    }
    if !testutil::docker::image_available("shell-exec-test") {
        return Err("shell-exec-test image not found; run: docker build -t shell-exec-test tests/docker/".into());
    }
    Ok(())
}
