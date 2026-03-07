//! Shared helpers included by multiple test binaries via `mod common;`.
//! Not every binary uses every function, so unused warnings are expected.
#![allow(dead_code)]

pub mod docker;

use thaum::ast::*;
use thaum::{parse, parse_with, Dialect};

pub fn parse_ok(input: &str) -> Program {
    parse(input).unwrap_or_else(|e| panic!("parse failed for {input:?}: {e}"))
}

pub fn first_expr(input: &str) -> Expression {
    parse_ok(input).lines.into_iter().flatten().next().unwrap().expression
}

pub fn first_cmd(input: &str) -> Command {
    match first_expr(input) {
        Expression::Command(c) => c,
        other => panic!("expected Command, got {other:?}"),
    }
}

pub fn first_compound(input: &str) -> CompoundCommand {
    match first_expr(input) {
        Expression::Compound { body, .. } => body,
        other => panic!("expected Compound, got {other:?}"),
    }
}

pub fn first_compound_bash(input: &str) -> CompoundCommand {
    let prog = parse_with(input, Dialect::Bash).unwrap_or_else(|e| panic!("parse failed for {input:?}: {e}"));
    let expr = prog.lines.into_iter().flatten().next().unwrap().expression;
    match expr {
        Expression::Compound { body, .. } => body,
        other => panic!("expected Compound, got {other:?}"),
    }
}

pub fn extract_word(arg: &Argument) -> &Word {
    match arg {
        Argument::Word(w) => w,
        other => panic!("expected Argument::Word, got {other:?}"),
    }
}

pub fn word_literal(arg: &Argument) -> String {
    extract_word(arg)
        .parts
        .iter()
        .filter_map(|p| match p {
            Fragment::Literal(s) => Some(s.as_str()),
            _ => None,
        })
        .collect()
}
