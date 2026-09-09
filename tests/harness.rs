//! Harness tests — guard the test configuration that CI depends on.
//!
//! Separate from `infra` because nothing here builds a Docker image. `infra`
//! is excluded from the gating CI job precisely because it does; these tests
//! must run there, so they need a binary that is not excluded.

#[path = "common/mod.rs"]
mod common;

#[path = "harness/nextest_config.rs"]
mod nextest_config;

fn main() {
    skuld::run_all();
}
