//! Infrastructure tests — verify that test/bench machinery works correctly.
//!
//! Tests here catch infrastructure problems (Docker, callgrind, binary
//! availability) independently of functional test suites like gauntlet or parse.
//!
//! **These tests run in no CI job.** The gating job excludes `binary(infra)` so
//! that a required check does not depend on the Docker build cache, and no other
//! job selects them. They also do not pass unattended even when selected: each
//! builds an image in its own process, so four builds contend for one daemon and
//! the per-test timeout is applied to queued work. Both are tracked in #37, which
//! proposes serialising them with a nextest test group. Run them locally with
//! `cargo nextest run --features cli -E 'binary(infra)'`.

#[path = "common/mod.rs"]
mod common;

// Infrastructure test modules. Each uses #[skuld::test] with appropriate
// `requires` preconditions so tests skip gracefully when tools are unavailable.
#[path = "infra/bench_callgrind.rs"]
mod bench_callgrind;
#[path = "infra/docker.rs"]
mod docker;
#[path = "infra/preconditions.rs"]
pub mod preconditions;

fn main() {
    skuld::run_all();
}
