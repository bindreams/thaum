//! Crate entry point. Exposes `parse()` and `parse_with()` for converting shell
//! source text into a typed AST, plus the `exec`, `lexer`, and `parser` modules
//! for lower-level access.

/// Typed abstract syntax tree for POSIX sh and Bash.
pub mod ast;
/// Callgrind output file parser for instruction-count benchmarks.
pub mod callgrind_parser;
/// Feature-flag system for shell dialect differences (POSIX vs. Bash).
pub mod dialect;
/// Lexer and parser error types with source spans.
pub mod error;
/// Shell executor: walks the AST and runs commands.
pub mod exec;
/// Ownership-based AST rewriting (fold / catamorphism).
pub mod fold;
/// AST formatting as YAML for structured output.
pub mod format;
/// Context-free shell tokenizer with buffered token stream.
pub mod lexer;
/// Recursive-descent parser that promotes keywords from grammatical context.
pub mod parser;
/// Byte-offset source spans for error reporting.
pub mod span;
/// Column-aligned table formatter for terminal output.
pub mod table;
/// Token types emitted by the lexer.
pub mod token;
/// Immutable AST visitor (walk the tree without modifying it).
pub mod visit;
/// Word expansion helpers (brace-param parsing, command substitution).
pub mod word;

pub use ast::Program;
pub use dialect::{Dialect, ShellOptions};
pub use error::ParseError;

/// Parse a complete shell program from source text (POSIX mode).
pub fn parse(input: &str) -> Result<Program, ParseError> {
    parse_with(input, Dialect::Posix)
}

/// Parse a complete shell program with the given dialect.
pub fn parse_with(input: &str, dialect: Dialect) -> Result<Program, ParseError> {
    parse_with_options(input, dialect.options())
}

/// Parse with explicit shell options (not a named dialect).
pub fn parse_with_options(input: &str, options: ShellOptions) -> Result<Program, ParseError> {
    parser::parse_with_options(input, options)
}
