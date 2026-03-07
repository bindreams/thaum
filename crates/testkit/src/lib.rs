//! Test and benchmark infrastructure for thaum.
//!
//! Contains `.sh.yaml` format parsing, Docker helpers, callgrind output parsing,
//! and cross-platform test tool fixtures. Consumed by integration tests and
//! benchmarks — not part of the public thaum library API.

pub mod callgrind_parser;
pub mod docker;
pub mod sh_yaml;
pub mod test_tools;

#[cfg(test)]
fn main() {
    skuld::run_all();
}
