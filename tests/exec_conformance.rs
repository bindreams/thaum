//! Conformance tests: compare our executor's output against real shells.
//!
//! These tests run the same script in our executor AND in Docker containers
//! running `dash` and `bash --posix`, then compare stdout and exit code.
//!
//! Requires: Docker installed and `shell-exec-test` image built.
//!
//! To run:
//!   docker build -t shell-exec-test tests/docker/
//!   cargo nextest run -P conformance --features cli
//!
//! Excluded from the default nextest profile (see .config/nextest.toml).

mod common;

#[path = "exec_conformance_support/mod.rs"]
mod support;

fn main() {
    testutil::run_all();
}
