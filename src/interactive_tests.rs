//! Unit tests for interactive mode pure logic: incomplete-input detection,
//! error classification, and history filtering.

use crate::interactive::is_incomplete;
use crate::Dialect;

// Incomplete-input detection ==========================================================================================

#[skuld::test]
fn incomplete_unterminated_double_quote() {
    let opts = Dialect::Posix.options();
    assert!(is_incomplete("echo \"hello", &opts));
}

#[skuld::test]
fn incomplete_unterminated_single_quote() {
    let opts = Dialect::Posix.options();
    assert!(is_incomplete("echo 'hello", &opts));
}

#[skuld::test]
fn incomplete_unclosed_if() {
    let opts = Dialect::Posix.options();
    assert!(is_incomplete("if true; then", &opts));
}

#[skuld::test]
fn incomplete_unclosed_while() {
    let opts = Dialect::Posix.options();
    assert!(is_incomplete("while true; do", &opts));
}

#[skuld::test]
fn trailing_backslash_parses_as_empty_string() {
    // The lexer treats a trailing backslash as a line continuation producing "",
    // so the parser sees a complete command. In the REPL, rustyline's multiline
    // mode handles the visual continuation — is_incomplete just checks the parse.
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("echo \\", &opts));
}

#[skuld::test]
fn heredoc_without_body_parses_with_empty_body() {
    // The lexer produces an empty heredoc body at EOF rather than an error,
    // so the parser sees a complete command.
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("cat <<EOF", &opts));
}

#[skuld::test]
fn incomplete_open_paren() {
    let opts = Dialect::Posix.options();
    assert!(is_incomplete("(echo hello", &opts));
}

#[skuld::test]
fn incomplete_open_brace() {
    let opts = Dialect::Posix.options();
    assert!(is_incomplete("{ echo hello", &opts));
}

#[skuld::test]
fn incomplete_unterminated_backquote() {
    let opts = Dialect::Posix.options();
    assert!(is_incomplete("echo `date", &opts));
}

#[skuld::test]
fn incomplete_unterminated_command_sub() {
    let opts = Dialect::Posix.options();
    assert!(is_incomplete("echo $(date", &opts));
}

// Complete input ======================================================================================================

#[skuld::test]
fn complete_simple_command() {
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("echo hello", &opts));
}

#[skuld::test]
fn complete_empty_input() {
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("", &opts));
}

#[skuld::test]
fn complete_if_fi() {
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("if true; then echo ok; fi", &opts));
}

#[skuld::test]
fn complete_quoted_string() {
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("echo 'complete'", &opts));
}

#[skuld::test]
fn complete_pipeline() {
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("ls | grep foo", &opts));
}

// Genuine syntax errors are NOT incomplete ============================================================================

#[skuld::test]
fn invalid_fi_alone_is_not_incomplete() {
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("fi", &opts));
}

#[skuld::test]
fn invalid_unexpected_token_is_not_incomplete() {
    let opts = Dialect::Posix.options();
    assert!(!is_incomplete("if true; then fi", &opts));
}

// Bash-specific =======================================================================================================

#[skuld::test]
fn incomplete_unclosed_double_bracket() {
    let opts = Dialect::Bash.options();
    assert!(is_incomplete("[[ -n hello", &opts));
}

// History filtering ===================================================================================================

use crate::interactive::should_save_to_history;

#[skuld::test]
fn histcontrol_ignorespace_filters_leading_space() {
    assert!(!should_save_to_history(" secret", "ignorespace", None));
}

#[skuld::test]
fn histcontrol_ignorespace_allows_normal() {
    assert!(should_save_to_history("ls", "ignorespace", None));
}

#[skuld::test]
fn histcontrol_ignoredups_filters_consecutive() {
    assert!(!should_save_to_history("ls", "ignoredups", Some("ls")));
}

#[skuld::test]
fn histcontrol_ignoredups_allows_different() {
    assert!(should_save_to_history("pwd", "ignoredups", Some("ls")));
}

#[skuld::test]
fn histcontrol_ignoreboth_filters_space_and_dups() {
    assert!(!should_save_to_history(" secret", "ignoreboth", None));
    assert!(!should_save_to_history("ls", "ignoreboth", Some("ls")));
    assert!(should_save_to_history("pwd", "ignoreboth", Some("ls")));
}

#[skuld::test]
fn histcontrol_none_saves_everything() {
    assert!(should_save_to_history(" secret", "", None));
    assert!(should_save_to_history("ls", "", Some("ls")));
}

#[skuld::test]
fn histcontrol_empty_line_never_saved() {
    assert!(!should_save_to_history("", "", None));
}

// Completion context detection ========================================================================================

use crate::interactive::{find_completion_context, CompletionContext};

#[skuld::test]
fn context_command_at_start() {
    assert_eq!(find_completion_context("", 0), CompletionContext::Command);
}

#[skuld::test]
fn context_command_partial() {
    assert_eq!(find_completion_context("ec", 2), CompletionContext::Command);
}

#[skuld::test]
fn context_argument_after_command() {
    assert_eq!(find_completion_context("ls ", 3), CompletionContext::Argument);
}

#[skuld::test]
fn context_argument_partial() {
    assert_eq!(find_completion_context("ls fo", 5), CompletionContext::Argument);
}

#[skuld::test]
fn context_command_after_pipe() {
    assert_eq!(find_completion_context("ls | ", 5), CompletionContext::Command);
}

#[skuld::test]
fn context_command_after_semicolon() {
    assert_eq!(find_completion_context("ls; ", 4), CompletionContext::Command);
}

#[skuld::test]
fn context_command_after_and() {
    assert_eq!(find_completion_context("ls && ", 6), CompletionContext::Command);
}

#[skuld::test]
fn context_variable_after_dollar() {
    assert_eq!(find_completion_context("echo $HO", 8), CompletionContext::Variable);
}

#[skuld::test]
fn context_variable_dollar_alone() {
    assert_eq!(find_completion_context("echo $", 6), CompletionContext::Variable);
}
