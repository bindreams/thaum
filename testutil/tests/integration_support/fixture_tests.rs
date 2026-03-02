//! Tests for the fixture system.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use std::path::Path;

use testutil::TempDir;

// A simple fixture that tracks setup/drop calls.

static COUNTER_SETUP_COUNT: AtomicU32 = AtomicU32::new(0);
static COUNTER_DROP_CALLED: AtomicBool = AtomicBool::new(false);

pub struct Counter {
    pub value: u32,
}

impl Drop for Counter {
    fn drop(&mut self) {
        COUNTER_DROP_CALLED.store(true, Ordering::Relaxed);
    }
}

#[testutil::fixture()]
fn counter() -> Result<Counter, String> {
    let n = COUNTER_SETUP_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(Counter { value: n + 1 })
}

// A fixture with a requirement that always fails.

fn unavailable_dep() -> Result<(), String> {
    Err("dep not available".into())
}

pub struct UnavailableFixture;

#[testutil::fixture(unavailable_dep)]
fn unavailable_fixture() -> Result<UnavailableFixture, String> {
    panic!("setup should never be called for unavailable fixture");
}

// Tests -----------------------------------------------------------------------

#[testutil::test]
fn fixture_per_test_is_fresh(#[fixture] c1: &Counter, #[fixture] c2: &Counter) {
    // Per-test fixtures: each #[fixture] param gets a fresh instance.
    assert_ne!(c1.value, c2.value, "per-test fixture should create fresh instances");
}

/// Fixture requirements propagate: a test using `UnavailableFixture` should be
/// skipped (not panicked) even without listing `unavailable_dep` in `#[requires]`.
#[testutil::test]
fn fixture_requirement_propagation(#[fixture] _f: &UnavailableFixture) {
    panic!("should never run — UnavailableFixture has an unmet requirement");
}

// TestName fixture ------------------------------------------------------------

#[testutil::test]
fn test_name_returns_function_name(#[fixture] name: &testutil::TestName) {
    assert_eq!(
        &**name, "test_name_returns_function_name",
        "TestName should return the test function name"
    );
}

#[testutil::test]
fn test_name_deref_to_str(#[fixture(testutil::TestName)] name: &str) {
    assert_eq!(
        name, "test_name_deref_to_str",
        "TestName with deref coercion should work as &str"
    );
}

// TempDir fixture -------------------------------------------------------------

#[testutil::test]
fn temp_dir_exists(#[fixture(TempDir)] dir: &Path) {
    assert!(dir.exists(), "temp dir should exist");
    assert!(dir.is_dir(), "temp dir should be a directory");
}

#[testutil::test]
fn temp_dir_contains_test_name(#[fixture(TempDir)] dir: &Path) {
    let dir_name = dir.file_name().unwrap().to_string_lossy();
    assert!(
        dir_name.starts_with("temp_dir_contains_test_name-"),
        "temp dir name should contain the test name, got: {dir_name}"
    );
}

#[testutil::test]
fn temp_dir_is_unique(#[fixture(TempDir)] d1: &Path, #[fixture(TempDir)] d2: &Path) {
    assert_ne!(d1, d2, "each per-test fixture injection should be a fresh instance");
}

// Post-run assertion ----------------------------------------------------------

pub fn assert_fixture_drop_called() {
    assert!(
        COUNTER_DROP_CALLED.load(Ordering::Relaxed),
        "Counter::drop should have been called during tests"
    );
}
