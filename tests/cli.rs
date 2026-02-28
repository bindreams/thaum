//! CLI tests: verify the thaum command-line interface.

#[path = "common/mod.rs"]
mod common;

#[path = "cli/output.rs"]
mod output;

fn main() {
    testutil::run_all();
}

testutil::default_labels!(lex, parse, cli);
