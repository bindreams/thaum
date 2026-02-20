# Shell Parser Development Guidelines

## Test-Driven Development
Write tests before or alongside implementation. Do not verify things by manually running them — write tests instead. When adding a new feature, write a failing test first, then implement until it passes.

## Correctness
- **Tests must match shell specification.** When writing tests, verify that the expected AST matches what bash/POSIX sh would actually execute. When in doubt, test in a real shell first.
- **When you notice an issue that won't be fixed immediately, write a TODO comment.** Comments are free. Include a description of the bug and what the correct behavior should be.

## Contracts and Assertions
Use the `contracts` crate and `debug_assert!` to verify pre-conditions, post-conditions, and internal invariants. These checks run in debug/test builds only — zero overhead in release.

- **`#[debug_requires(condition)]`** — precondition on a function. Use when the condition can be expressed over the function's parameters and `self`.
- **`#[debug_ensures(condition)]`** — postcondition on a function. Use when the condition can be expressed over `self` and `ret` (the return value). Supports `old()` to capture pre-call state.
- **`debug_assert!(condition)`** — inline assertion for conditions that don't fit attribute syntax (conditional checks, iteration over collections, mid-function invariants).

Place contracts on every function where there is a meaningful invariant to check. Contracts are documentation that also catches regressions. Contracts and assertions are free in debug builds — in debug mode, safety and correctness are more important than performance. Add checks liberally.

## Interface Design

1. **Method names reflect intent.** Observing actions use prefixes like `is_*`, `can_*`, `peek_*`, `as_*`. Mutating actions use verbs like `advance`, `skip`, `scan`, `push`, `pop`.
2. **Observing actions must not call mutating actions.** The only exception is mutating an internal cache, which must be wrapped in a `Cell`/`RefCell`.
3. **Query logic belongs on the data, not the consumer.** Token-level queries (is this a keyword? can a command start here?) are methods on `Token`. The parser peeks tokens itself and calls Token methods — it does not wrap peek+query in its own methods.
4. **Token ownership.** A parsing function consumes only the tokens that constitute the AST node it creates. Leading whitespace is the caller's responsibility to skip; trailing whitespace is the next consumer's responsibility. For example, `collect_word()` does not skip whitespace before or after — the caller handles word boundaries. Helper methods (`eat`, `expect`, `expect_keyword`, `expect_closing_keyword`) skip whitespace internally because they are boundary utilities, not AST-building functions.

## Architecture

See CONTRIBUTING.md for detailed architecture (AST naming, operator precedence, dialect system, adding new features).

### Lexer/Parser pipeline
- The **lexer is context-free** — it produces fragment tokens (`Literal`, `SimpleParam`, `DoubleQuoted`, etc.), `Whitespace`, `IoNumber`, operators, `Newline`, `HereDocBody`, and `Eof`. It never promotes words to reserved word tokens.
- The **parser promotes keywords** — it checks `Token::Literal("if")` etc. when the grammatical context expects a keyword.
- The **lexer has no lifetime parameter** — it owns a `CharSource` backed by `Read`. Constructed via `Lexer::from_str()` or `Lexer::from_reader()`.
- The **parser holds the lexer directly** — no separate TokenStream layer.
- **`speculate()`** on the Lexer saves `buf_pos`, runs a closure, and rewinds on failure. Tokens scanned during speculation stay in the buffer — scanning state is purely cursor-side and doesn't need saving.
- **`LastScanned`** — one-token lookbehind enum (`Fragment`, `Whitespace`, `Other`) that governs whitespace significance, process substitution detection (`<(`/`>(` only after whitespace), and tilde prefix recognition. `Whitespace` tokens are only emitted when significant (between fragments).
- The lexer handles heredocs autonomously — no parser→lexer feedback.

### Token naming
- Bash-specific tokens use `Bash` prefix: `BashHereStringOp`, `BashDblLBracket`, etc.
- Semantic names, not character mnemonics: `RedirectFromFile` not `Less`
- POSIX spec names in doc comments: `/// '<' — redirect input (POSIX: LESS)`

### Project structure
- `src/lexer/` — context-free tokenizer (char_source, heredoc, operators, word_scan)
- `src/parser/` — recursive descent (expressions, commands, compound, bash, helpers)
- `src/word/` — word expansion parsing (fragments, params, substitution)
- `src/exec/` — runtime evaluation (arithmetic)
- `src/cli/` — CLI binary (yaml_writer, error_fmt, source_map, color)
- `tests/` — split by topic: commands, pipelines, compound, redirects, errors, word_expansion, bash_features, cli_output, exec_basic, exec_conformance
