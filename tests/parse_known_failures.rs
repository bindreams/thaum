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
// 2. Heredoc in pipeline / condition — FIXED (see tests/redirects.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 3. << inside (( )) — FIXED (see tests/bash_features.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 4. (( paren ambiguity — FIXED (see tests/bash_features.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 5. Arithmetic features — FIXED (see tests/bash_features.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 6. [[ edge cases — FIXED (see tests/bash_features.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 7. { without space — NOT A BUG. Bash also rejects `{ls; }` (status 2).
//    `{` requires whitespace to be a keyword; `{ls;` is a word, but `}` alone
//    in command position is a syntax error in both bash and our parser.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 8. case inside $() — FIXED (see tests/commands.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 9. Quoting edge cases
//    - ansi_c_backslash_c: NOT A BUG (bash also rejects $'\c'' as unterminated quote)
//    - glob_with_closing_bracket_and_quotes: FIXED (see tests/bash_features.rs)
// ---------------------------------------------------------------------------

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
