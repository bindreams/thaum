//! Integration tests for testutil: verifies `#[requires]` macro behavior.

#[path = "integration_support/mod.rs"]
mod support;

fn main() {
    let conclusion = testutil::run_tests();

    // Post-run assertion: verify that the satisfied-precondition test actually
    // executed its body (not silently skipped as "ok").
    support::harness_tests::assert_satisfied_test_ran();

    conclusion.exit();
}
