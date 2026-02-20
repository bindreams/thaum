pub mod ast;
pub mod dialect;
pub mod error;
pub mod exec;
pub mod format;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod token;
pub mod word;

pub use ast::Program;
pub use dialect::{Dialect, ParseOptions};
pub use error::ParseError;

/// Parse a complete shell program from source text (POSIX mode).
pub fn parse(input: &str) -> Result<Program, ParseError> {
    parse_with(input, Dialect::Posix)
}

/// Parse a complete shell program with the given dialect.
pub fn parse_with(input: &str, dialect: Dialect) -> Result<Program, ParseError> {
    parser::parse_with_options(input, dialect.options())
}
