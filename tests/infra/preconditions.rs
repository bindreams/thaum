//! Precondition functions for infrastructure tests.

use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn valgrind() -> Result<(), String> {
    Command::new("valgrind")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
        .then_some(())
        .ok_or_else(|| "valgrind not installed".into())
}

pub fn thaum_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/thaum")
}

pub fn thaum() -> Result<(), String> {
    let path = thaum_binary_path();
    if path.exists() {
        Ok(())
    } else {
        Err(format!("thaum binary not found at {}", path.display()))
    }
}

pub fn docker() -> Result<(), String> {
    if thaum_testkit::docker::available() {
        Ok(())
    } else {
        Err("Docker not available".into())
    }
}
