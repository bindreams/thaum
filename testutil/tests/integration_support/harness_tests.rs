//! Tests for the `#[requires]` macro and the testutil harness.

use std::sync::atomic::{AtomicBool, Ordering};

use testutil::requires;

fn always_ok() -> Result<(), String> {
    Ok(())
}

fn always_fail() -> Result<(), String> {
    Err("intentionally unavailable".into())
}

static SATISFIED_TEST_RAN: AtomicBool = AtomicBool::new(false);

#[requires(always_ok)]
fn requires_satisfied_runs_body() {
    SATISFIED_TEST_RAN.store(true, Ordering::Relaxed);
}

/// Called after `run_all()` to verify the body actually executed.
pub fn assert_satisfied_test_ran() {
    assert!(
        SATISFIED_TEST_RAN.load(Ordering::Relaxed),
        "requires_satisfied_runs_body should have executed"
    );
}

#[requires(always_fail)]
fn requires_unsatisfied_skips_body() {
    panic!("this body should never execute");
}

#[requires(always_ok, always_fail)]
fn requires_partial_failure_skips_body() {
    panic!("this body should never execute when any requirement fails");
}
