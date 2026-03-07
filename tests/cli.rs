//! CLI tests: verify the thaum command-line interface.

#[path = "common/mod.rs"]
mod common;

#[path = "cli/interactive.rs"]
mod interactive;
#[path = "cli/output.rs"]
mod output;

fn main() {
    skuld::run_all();
}

skuld::default_labels!(lex, parse, cli);
