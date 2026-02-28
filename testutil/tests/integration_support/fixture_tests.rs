//! Tests for the fixture system.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use testutil::{requires, Fixture, Scope};

// A simple fixture that tracks setup/teardown calls.

static COUNTER_SETUP_COUNT: AtomicU32 = AtomicU32::new(0);
static COUNTER_TEARDOWN_CALLED: AtomicBool = AtomicBool::new(false);

pub struct Counter {
    pub value: u32,
}

#[testutil::fixture()]
impl Fixture for Counter {
    const SCOPE: Scope = Scope::Static;

    fn setup() -> Result<Self, String> {
        let n = COUNTER_SETUP_COUNT.fetch_add(1, Ordering::Relaxed);
        Ok(Counter { value: n + 1 })
    }

    fn teardown(&self) {
        COUNTER_TEARDOWN_CALLED.store(true, Ordering::Relaxed);
    }
}

// A fixture with a requirement that always fails.

fn unavailable_dep() -> Result<(), String> {
    Err("dep not available".into())
}

pub struct UnavailableFixture;

#[testutil::fixture(unavailable_dep)]
impl Fixture for UnavailableFixture {
    const SCOPE: Scope = Scope::Static;

    fn setup() -> Result<Self, String> {
        panic!("setup should never be called for unavailable fixture");
    }
}

// Tests -----------------------------------------------------------------------

#[requires()]
fn fixture_static_is_shared(#[fixture] c1: &Counter, #[fixture] c2: &Counter) {
    // Both references should point to the same value (setup called once).
    assert_eq!(c1.value, c2.value, "static fixture should be shared");
}

#[requires()]
fn fixture_setup_called_once(#[fixture] _c: &Counter) {
    // Setup should have been called exactly once across all tests.
    assert_eq!(
        COUNTER_SETUP_COUNT.load(Ordering::Relaxed),
        1,
        "static fixture setup should be called exactly once"
    );
}

/// Fixture requirements propagate: a test using `UnavailableFixture` should be
/// skipped (not panicked) even without listing `unavailable_dep` in `#[requires]`.
#[requires()]
fn fixture_requirement_propagation(#[fixture] _f: &UnavailableFixture) {
    panic!("should never run — UnavailableFixture has an unmet requirement");
}

pub fn assert_fixture_teardown_called() {
    assert!(
        COUNTER_TEARDOWN_CALLED.load(Ordering::Relaxed),
        "Counter::teardown should have been called after tests"
    );
}
