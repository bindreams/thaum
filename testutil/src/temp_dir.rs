//! Per-test temporary directory fixture, named after the current test.

use std::ops::Deref;
use std::path::Path;

use crate::TestName;

/// A per-test temporary directory. Created fresh for each test, removed on drop.
///
/// The directory name includes the test function name for easier debugging.
/// Implements `Deref<Target = Path>` so it can be used as `&Path` directly
/// via `#[fixture(TempDir)] dir: &Path`.
pub struct TempDir {
    inner: tempfile::TempDir,
}

impl Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.inner.path()
    }
}

#[testutil::fixture()]
fn temp_dir(#[fixture(TestName)] name: &str) -> Result<TempDir, String> {
    tempfile::Builder::new()
        .prefix(&format!("{name}-"))
        .tempdir()
        .map(|inner| TempDir { inner })
        .map_err(|e| format!("failed to create temp dir: {e}"))
}
