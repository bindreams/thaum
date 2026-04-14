//! Command substitution parsing.

use crate::ast::Statement;
use crate::dialect::ShellOptions;
use crate::error::ParseError;

/// Parse a command substitution body into statements by invoking the full
/// parser with the supplied dialect options. Errors from the inner parse
/// are propagated.
///
/// TODO: error spans returned here are relative to the inner string slice,
/// not the outer source. Adjusting offsets would require threading an
/// absolute offset through every span constructor in the recursive parse —
/// a separate, larger refactor.
pub(crate) fn parse_command_substitution(cmd: &str, options: ShellOptions) -> Result<Vec<Statement>, ParseError> {
    let prog = crate::parser::parse_with_options(cmd, options)?;
    Ok(prog.lines.into_iter().flatten().collect())
}
