//! Infrastructure tests — verify that test/bench machinery works correctly.
//!
//! Tests here catch infrastructure problems (Docker, callgrind, binary
//! availability) independently of functional test suites like corpus or parse.

#[path = "common/mod.rs"]
mod common;

// Infrastructure test modules. Each uses #[testutil::test] with appropriate
// `requires` preconditions so tests skip gracefully when tools are unavailable.
#[path = "infra/bench_callgrind.rs"]
mod bench_callgrind;
#[path = "infra/docker.rs"]
mod docker;
#[path = "infra/preconditions.rs"]
pub mod preconditions;

fn main() {
    testutil::run_all();
}
