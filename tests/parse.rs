//! Parse tests: verify AST structure produced by the parser.

#[path = "common/mod.rs"]
mod common;

#[path = "parse/bash.rs"]
mod bash;
#[path = "parse/commands.rs"]
mod commands;
#[path = "parse/compound.rs"]
mod compound;
#[path = "parse/errors.rs"]
mod errors;
#[path = "parse/pipelines.rs"]
mod pipelines;
#[path = "parse/redirects.rs"]
mod redirects;
#[path = "parse/word_expansion.rs"]
mod word_expansion;

fn main() {
    skuld::run_all();
}

skuld::default_labels!(lex, parse);
