//! Command substitution parsing.

use crate::ast::Statement;

/// Parse a command substitution body into statements by invoking the full parser.
pub(crate) fn parse_command_substitution(cmd: &str) -> Vec<Statement> {
    crate::parser::parse(cmd)
        .map(|prog| prog.statements)
        .unwrap_or_default()
}
