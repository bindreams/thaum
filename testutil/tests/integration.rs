//! Integration tests for testutil: verifies `#[requires]` macro behavior.

#[path = "integration_support/mod.rs"]
mod support;

fn main() {
    let conclusion = testutil::run_tests();

    // Post-run assertions: verify test bodies and teardowns actually ran.
    support::harness_tests::assert_satisfied_test_ran();
    support::fixture_tests::assert_fixture_teardown_called();

    conclusion.exit();
}
