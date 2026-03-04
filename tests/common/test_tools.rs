//! Cross-platform test tool fixture.
//!
//! Provides a process-scoped `test_tools` fixture that returns a directory
//! containing symlinks (Unix) or copies (Windows) of minimal tool binaries
//! (`echo`, `true`, `false`, `cat`, `env`, `sh`). Tests use this directory
//! as `PATH` to avoid depending on system binaries.

use std::ops::Deref;
use std::path::{Path, PathBuf};

/// Directory containing cross-platform test tool binaries.
pub struct TestTools {
    dir: PathBuf,
}

impl Deref for TestTools {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TestTools {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Return the platform-appropriate executable name (adds `.exe` on Windows).
fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Find the cargo target directory where built binaries live.
fn cargo_bin_dir() -> PathBuf {
    std::env::current_exe()
        .expect("current_exe()")
        .parent()
        .expect("deps/")
        .parent()
        .expect("target/debug or target/release")
        .to_path_buf()
}

/// Create a symlink (Unix) or copy (Windows) from `src` to `dst`.
fn link_or_copy(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!(
            "test tool binary not found: {}. Run `cargo build` first.",
            src.display()
        ));
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst).map_err(|e| format!("symlink {} -> {}: {e}", dst.display(), src.display()))
    }

    #[cfg(windows)]
    {
        std::fs::copy(src, dst)
            .map(|_| ())
            .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err("unsupported platform".into())
    }
}

#[skuld::fixture(scope = process, deref)]
fn test_tools() -> Result<TestTools, String> {
    let bin_dir = cargo_bin_dir();
    let dir = tempfile::Builder::new()
        .prefix("thaum-test-tools-")
        .tempdir()
        .map_err(|e| format!("create test_tools dir: {e}"))?;
    let dir = dir.keep(); // Persist — cleanup via TestTools::drop

    let tools: &[(&str, &str)] = &[
        ("echo", "test-echo"),
        ("true", "test-true"),
        ("false", "test-false"),
        ("cat", "test-cat"),
        ("env", "test-env"),
        ("pwd", "test-pwd"),
        ("touch", "test-touch"),
        ("sh", "thaum"), // thaum impersonates sh when invoked as "sh"
    ];

    for &(name, bin_name) in tools {
        let src = bin_dir.join(exe_name(bin_name));
        let dst = dir.join(exe_name(name));
        link_or_copy(&src, &dst)?;
    }

    Ok(TestTools { dir })
}
