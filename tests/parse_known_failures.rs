//! Minimal reproductions of known parser limitations.
//!
//! Each test demonstrates valid shell syntax that our parser incorrectly rejects.
//! All tests are `#[ignore]` — run with `cargo test --test parse_known_failures -- --ignored`.
//!
//! When a bug is fixed, un-ignore the corresponding test so it guards against regressions.

use thaum::{parse, parse_with, Dialect};

// ---------------------------------------------------------------------------
// 1. Empty compound bodies — FIXED (see tests/compound.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 2. Heredoc in pipeline / condition
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn heredoc_in_if_condition() {
    // Heredoc body appears between the condition line and `then`.
    parse("if cat <<EOF; then\nhello\nEOF\necho yes\nfi").unwrap();
}

#[test]
#[ignore]
fn heredoc_with_pipe_on_last_line() {
    // Pipe on the heredoc-triggering line; body before the next command.
    parse("cat <<EOF |\n1\n2\nEOF\ntac").unwrap();
}

#[test]
#[ignore]
fn multiple_heredocs_in_pipeline() {
    parse("cat <<A |\na\nA\ncat <<B\nb\nB").unwrap();
}

// ---------------------------------------------------------------------------
// 3. << inside (( )) parsed as heredoc instead of left-shift
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn double_paren_shift_not_heredoc() {
    // << inside (( )) is left-shift, not a heredoc operator.
    parse_with("(( 1 << 32 ))\necho ok", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn c_style_for_with_shift() {
    // << inside for (( )) is left-shift, not a heredoc.
    // NOTE: the single-line form `for ((i = 1 << 32; ...))` parses fine;
    // the bug triggers when other statements precede the for loop.
    parse_with(
        "x=0\n\nfor ((i = 1 << 32; i; ++i)); do\nbreak\ndone",
        Dialect::Bash,
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 4. (( paren ambiguity — arithmetic vs subshell-of-subshell
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn subshell_starting_with_path() {
    // ((/path/cmd ...)) — (( followed by / means subshell, not arithmetic.
    // In bash mode, (( triggers arithmetic parsing; the / disambiguates.
    parse_with("((/usr/bin/cat </dev/zero; echo hi) | true)", Dialect::Bash).unwrap();
}

// ---------------------------------------------------------------------------
// 5. Arithmetic features — syntax inside (( )) and $(( ))
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn arith_empty_expression() {
    // (( )) is valid bash — evaluates to 0 (exit status 1).
    parse_with("(( ))\necho ok", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn arith_literal_subscript() {
    // 1[2] — a literal with subscript. Should parse; runtime error, not parse error.
    parse_with("(( 1[2] = 3 ))", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn arith_single_quoted_value() {
    // Single-quoted string as rhs inside (( )).
    parse_with("(( A['y'] = 'y' ))", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn arith_command_sub() {
    // $() inside (( )) — the $ before ( must not be confused.
    parse_with("(( a = $(echo 1) + 2 ))", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn arith_dollar_positional() {
    // $N (positional parameter) inside (( )).
    parse_with("(( A[$key] += $2 ))", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn arith_redirect_after_dparen() {
    // Redirect after (( )) — the $() inside triggers the bug.
    parse_with("(( a = $(echo 42) + 10 )) 2>/dev/null", Dialect::Bash).unwrap();
}

// ---------------------------------------------------------------------------
// 6. [[ edge cases
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn double_bracket_close_not_after_open() {
    // ]] when not preceded by [[ — e.g. from variable expansion.
    // $dbracket expands to [[ at runtime; ]] should be a regular word.
    parse_with("dbracket=[[\n$dbracket foo == foo ]]", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn glob_posix_char_class() {
    // [[:punct:]] is a POSIX character class in a glob, not [[ ]].
    parse_with("echo *.[[:punct:]]", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn regex_with_parens_in_double_bracket() {
    // Parenthesized regex pattern inside [[ =~ ]].
    parse_with("[[ (foo =~ bar) ]]", Dialect::Bash).unwrap();
}

// ---------------------------------------------------------------------------
// 7. { without space — not a keyword
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn brace_without_space_is_command() {
    // { is only a keyword when followed by whitespace. {ls; is a command name.
    parse("{ls; }").unwrap();
}

// ---------------------------------------------------------------------------
// 8. case inside $()
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn case_in_command_sub() {
    // ) in a case pattern inside $() must not close the command substitution.
    parse("echo $(case x in a) echo yes;; esac)").unwrap();
}

// ---------------------------------------------------------------------------
// 9. Quoting edge cases
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn ansi_c_backslash_c_terminates_quote() {
    // In some shells \c inside $'...' terminates the string.
    // The trailing ' opens a new single-quoted string that contains ` | cat`.
    parse_with("echo -n $'\\c'' | cat", Dialect::Bash).unwrap();
}

#[test]
#[ignore]
fn glob_with_closing_bracket_and_quotes() {
    // Quotes inside bracket expressions in globs.
    parse_with("echo [hello\"]\"", Dialect::Bash).unwrap();
}

// ---------------------------------------------------------------------------
// 10. Nested function declaration
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn nested_function_declaration() {
    // bash allows: f() g() { echo hi; }
    parse_with("f() g() { echo hi; }", Dialect::Bash).unwrap();
}

// ---------------------------------------------------------------------------
// 11. Miscellaneous
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn function_keyword_at_eof() {
    // `function foo` at end of input — incomplete construct.
    parse_with("foo=bar\nfunction foo", Dialect::Bash).unwrap();
}
