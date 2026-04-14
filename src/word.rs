//! Word expansion helpers.
//!
//! Provides `parse_brace_param_content` for parsing `${VAR:-default}` internals,
//! and `parse_command_substitution` for recursive parsing of `$(...)` bodies.
//! Fragment splitting is handled by the lexer; these helpers handle the
//! internal structure of individual expansion types. The free function
//! `token_to_fragment` (in `fragment.rs`) is shared between the parser and
//! the executor's gettext re-lex path.

mod fragment;
mod params;
mod subst;

pub(crate) use fragment::{merge_adjacent_literals, token_to_fragment};
pub(crate) use params::parse_brace_param_content;
pub(crate) use subst::parse_command_substitution;
