//! Precondition functions for infrastructure tests.

use std::path::PathBuf;

pub fn valgrind() -> Result<(), String> {
    testutil::probe_executable("valgrind")
}

pub fn thaum_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/thaum")
}

pub fn thaum() -> Result<(), String> {
    testutil::probe_path(thaum_binary_path())
}

pub fn docker() -> Result<(), String> {
    if testutil::docker::available() {
        Ok(())
    } else {
        Err("Docker not available".into())
    }
}
