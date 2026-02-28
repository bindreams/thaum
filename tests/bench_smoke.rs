//! Smoke tests for the benchmark infrastructure.
//!
//! Uses testutil's `#[requires]` harness so that tests needing external tools
//! (valgrind, thaum binary) show as `ignored` with an unavailability summary.

#[path = "bench_smoke_support/mod.rs"]
mod support;

fn main() {
    testutil::run_all();
}
