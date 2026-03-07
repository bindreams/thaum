//! Pure logic for interactive shell mode: incomplete-input detection
//! and error classification. These functions have no I/O dependencies and
//! are fully unit-testable.

use crate::error::{LexError, ParseError};
use crate::ShellOptions;

/// Check whether the given input is incomplete (needs more lines).
///
/// Attempts to parse `input` and classifies any resulting error. Returns `true`
/// only when the error indicates unterminated constructs — not for genuine syntax
/// errors like unexpected tokens.
pub fn is_incomplete(input: &str, options: &ShellOptions) -> bool {
    if input.is_empty() {
        return false;
    }

    match crate::parse_with_options(input, options.clone()) {
        Ok(_) => false,
        Err(e) => classify_as_incomplete(&e),
    }
}

/// Returns `true` if the parse error indicates the input is incomplete rather
/// than genuinely malformed.
fn classify_as_incomplete(error: &ParseError) -> bool {
    match error {
        ParseError::Lex(lex) => matches!(
            lex,
            LexError::UnterminatedSingleQuote { .. }
                | LexError::UnterminatedDoubleQuote { .. }
                | LexError::UnterminatedHereDoc { .. }
                | LexError::UnterminatedBackquote { .. }
                | LexError::UnterminatedExpansion { .. }
        ),
        ParseError::UnexpectedEof { .. } => true,
        ParseError::UnclosedConstruct { .. } => true,
        ParseError::UnexpectedToken { found, .. } => found == "end of input",
    }
}

// History filtering ===================================================================================================

/// Check whether a command line should be saved to history based on HISTCONTROL.
///
/// `histcontrol` is the colon-separated value of `$HISTCONTROL`. Supported
/// values: `ignorespace`, `ignoredups`, `ignoreboth`.
pub fn should_save_to_history(line: &str, histcontrol: &str, prev_line: Option<&str>) -> bool {
    if line.is_empty() {
        return false;
    }

    for control in histcontrol.split(':') {
        match control.trim() {
            "ignorespace" => {
                if line.starts_with(' ') {
                    return false;
                }
            }
            "ignoredups" => {
                if let Some(prev) = prev_line {
                    if line == prev {
                        return false;
                    }
                }
            }
            "ignoreboth" => {
                if line.starts_with(' ') {
                    return false;
                }
                if let Some(prev) = prev_line {
                    if line == prev {
                        return false;
                    }
                }
            }
            _ => {}
        }
    }

    true
}

// Completion context detection ========================================================================================

/// What kind of token the user is completing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionContext {
    /// Completing a command name (first word, or after `|`, `&&`, `||`, `;`).
    Command,
    /// Completing a command argument (file path, etc.).
    Argument,
    /// Completing a variable name (after `$`).
    Variable,
}

/// Determine what kind of completion is appropriate at `pos` in `line`.
///
/// NOTE: This is a best-effort heuristic. It does not account for quoting
/// (a `$` or `|` inside quotes will be misidentified). Good enough for basic
/// tab completion; a proper implementation would need a partial parse.
pub fn find_completion_context(line: &str, pos: usize) -> CompletionContext {
    let prefix = &line[..pos];

    // Check if we're in a $VARIABLE context
    if let Some(dollar_pos) = prefix.rfind('$') {
        let after_dollar = &prefix[dollar_pos + 1..];
        // If everything after $ is alphanumeric/underscore, it's a variable completion
        if after_dollar.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return CompletionContext::Variable;
        }
    }

    // Find the start of the current "simple command" by looking for operators
    let trimmed = prefix.trim_end();
    if trimmed.is_empty() {
        return CompletionContext::Command;
    }

    // Check if the last non-whitespace is a command separator
    if trimmed.ends_with('|') || trimmed.ends_with(';') || trimmed.ends_with("&&") || trimmed.ends_with("||") {
        return CompletionContext::Command;
    }

    // Count words in the current simple command (after last separator).
    // Take the maximum end-position across all separator types so that
    // a later || is not shadowed by an earlier &&.
    let last_sep = [
        trimmed.rfind("&&").map(|i| i + 2),
        trimmed.rfind("||").map(|i| i + 2),
        trimmed.rfind(['|', ';', '&']).map(|i| i + 1),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    let cmd_part = &trimmed[last_sep..].trim_start();
    let word_count = cmd_part.split_whitespace().count();

    if word_count <= 1 && !prefix.ends_with(' ') {
        CompletionContext::Command
    } else {
        CompletionContext::Argument
    }
}

#[cfg(test)]
#[path = "interactive_tests.rs"]
mod tests;
