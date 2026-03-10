//! Build script that compiles the test tool binaries.
//!
//! The binaries are compiled into a separate target directory inside `OUT_DIR`
//! to avoid deadlocking on the outer cargo's target directory lock. A recursion
//! guard env var (`THAUM_TEST_TOOLS_INNER_BUILD`) prevents infinite recursion
//! when the inner cargo invocation re-runs this build script.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Recursion guard: the inner `cargo build --bins` re-triggers this build
    // script for the same package. Cargo compiles the library even with --bins
    // (it's an implicit dependency), so we must set a dummy TEST_TOOLS_BIN_DIR
    // to satisfy `env!()`. The real value is set by the outer build below.
    if std::env::var_os("THAUM_TEST_TOOLS_INNER_BUILD").is_some() {
        println!("cargo:rustc-env=TEST_TOOLS_BIN_DIR=");
        return;
    }

    let cargo = std::env::var("CARGO").expect("CARGO env var");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR env var");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR env var");
    let profile = std::env::var("PROFILE").expect("PROFILE env var");

    let manifest_path = PathBuf::from(&manifest_dir).join("Cargo.toml");
    let inner_target_dir = format!("{out_dir}/thaum-test-tools.build");

    // Emit rerun-if-changed for each binary source listed in Cargo.toml.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    for path in bin_source_paths(&manifest_path) {
        println!("cargo:rerun-if-changed={path}");
    }

    // Build binaries with a separate target directory to avoid lock contention.
    let mut cmd = Command::new(&cargo);
    cmd.args(["build", "--bins"]);
    cmd.args(["--manifest-path", manifest_path.to_str().unwrap()]);
    cmd.args(["--target-dir", &inner_target_dir]);
    cmd.env("THAUM_TEST_TOOLS_INNER_BUILD", "1");
    if profile == "release" {
        cmd.arg("--release");
    }

    let status = cmd.status().expect("failed to run inner cargo build");
    assert!(status.success(), "inner cargo build failed with {status}");

    // Export the directory containing the built binaries.
    let bin_dir = format!("{inner_target_dir}/{profile}");
    println!("cargo:rustc-env=TEST_TOOLS_BIN_DIR={bin_dir}");
}

/// Parse `[[bin]]` sections from Cargo.toml and return the `path` values.
fn bin_source_paths(manifest_path: &PathBuf) -> Vec<String> {
    let content = std::fs::read_to_string(manifest_path).expect("read Cargo.toml");
    let mut paths = Vec::new();
    let mut in_bin_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[bin]]" {
            in_bin_section = true;
            continue;
        }
        // Any other section header ends the current [[bin]] block.
        if trimmed.starts_with('[') {
            in_bin_section = false;
            continue;
        }
        if in_bin_section && trimmed.starts_with("path") {
            if let Some(value) = trimmed.split('=').nth(1) {
                let path = value.trim().trim_matches('"');
                paths.push(path.to_string());
            }
        }
    }
    paths
}
