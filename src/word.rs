//! Word expansion helpers.
//!
//! Provides `parse_brace_param_content` for parsing `${VAR:-default}` internals,
//! and `parse_command_substitution` for recursive parsing of `$(...)` bodies.
//! Fragment splitting is handled by the lexer; these helpers handle the
//! internal structure of individual expansion types.

mod params;
mod subst;

pub(crate) use params::parse_brace_param_content;
pub(crate) use subst::parse_command_substitution;
