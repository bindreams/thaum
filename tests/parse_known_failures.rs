//! Minimal reproductions of known parser limitations.
//!
//! Each test demonstrates valid shell syntax that our parser incorrectly rejects.
//! All tests are `#[ignore]` — run with `cargo test --test parse_known_failures -- --ignored`.
//!
//! When a bug is fixed, un-ignore the corresponding test so it guards against regressions.

// All original categories have been resolved:
//  1. Empty compound bodies — FIXED (see tests/compound.rs)
//  2. Heredoc in pipeline / condition — FIXED (see tests/redirects.rs)
//  3. << inside (( )) — FIXED (see tests/bash_features.rs)
//  4. (( paren ambiguity — FIXED (see tests/bash_features.rs)
//  5. Arithmetic features — FIXED (see tests/bash_features.rs)
//  6. [[ edge cases — FIXED (see tests/bash_features.rs)
//  7. { without space — NOT A BUG (bash also rejects)
//  8. case inside $() — FIXED (see tests/commands.rs)
//  9. Quoting: ansi_c \c — NOT A BUG; glob+quotes — FIXED (see tests/bash_features.rs)
// 10. Nested function f() g() — NOT A BUG (bash also rejects)
// 11. function foo at EOF — NOT A BUG (bash also rejects)
