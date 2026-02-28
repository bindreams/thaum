# Shell Parser Development Guidelines

## Important rules
- **Important!** Follow TDD - test-driven development! In particular:
  - Write tests before implementing functionality of fixing bugs;
  - Write tests instead of or in addition to manually verifying.
- Avoid letting files grow too large. If tempted to add section comments in the file, always consider breaking it up into several, perhaps organized into a folder.
  - If a section comment is inevitable, use this style: `// Section name ======` with equal signs up to the 120th column, or `// Inner name -----` for a second level of sectioning.
- Use `foo.rs` + `foo/` module style instead of `mod.rs`.
- Debug asserts and contracts are encouraged and should be considered zero-cost.
- Adding a new crate to dependencies is always preferable to implementing functionality manually.
- Place unit tests in a separate file (e.g. `dir/foo_tests.rs` for `dir/foo.rs`).
- Group integration tests by the functionality they test, not by current task, origin, or failure state.
- Follow the documentation style in [CONTRIBUTING.md](/CONTRIBUTING.md#documentation-style). When writing comments and docstrings, think carefully about the context that the reader will have:
  - For module-level docstrings, the reader already knows about the file location within the project, and has read README.md;
  - For file-level docstrings, in addition to the above, the reader knows which module this file is in;
  - For function-level docstrings, in addition to the above, the reader has read the function name and argument names. Avoid duplicating obvious information, but include otherwise hidden knowledge.
  - The reader does not know what task you were accomplishing while writing this comment/docstring. Avoid including unnecessary task-related details.

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

## Test macro
Use `#[testutil::test]` instead of `#[test]` everywhere. All test binaries use `harness = false` with the testutil custom harness. Do NOT use bare `#[test]` — it compiles but silently never runs.

Optional arguments:
- `requires = [f1, f2]` — runtime preconditions (`fn() -> Result<(), String>`)
- `name = "display name"` — custom name in test output
- `labels = [docker, slow]` — prepended as `[docker][slow]` for nextest filtering
- `ignore` or `ignore = "reason"` — statically ignore the test

For dynamic test generation (e.g. corpus tests from data files), use `testutil::TestRunner::add()`.

## Pre-commit checklist
Before every commit, run `pre-commit run --all-files` and fix any issues. This checks:
1. No stray `#[test]` — use `#[testutil::test]` instead
2. `cargo fmt` — formatting is correct
3. `cargo clippy` — no linter warnings

Additionally:
4. Run `cargo nextest run --features cli`: all tests pass
5. Update stale information in documentation:
   - `README.md`: General information for new users
   - `CONTRIBUTING.md`: Guidance for contributors (people and LLMs)
   - `CLAUDE.md`: Instructions specifically for LLM agents

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
- `src/exec/` — runtime execution engine:
  - `exec.rs` — Executor struct, main dispatch, alias expansion
  - `environment.rs` — variables, functions, scoping, arrays, aliases, declare attrs
  - `builtins.rs` — builtin commands (echo, cd, test, declare, printf, etc.)
  - `special_builtins.rs` — eval, exec, source (need Executor access)
  - `arithmetic.rs` — arithmetic expression evaluation
  - `bash_test.rs` — `[[ ]]` conditional evaluator
  - `printf.rs` — printf builtin formatter (custom, not Rust format!)
  - `expand.rs` — word expansion (parameters, tilde, substitution)
  - `gettext.rs` — GNU gettext catalog lookup for `$"..."` locale translation
  - `compound.rs` — compound command execution (if/while/for/case)
  - `pipeline.rs` — pipeline execution
  - `external.rs` + `command_ex.rs` — external process spawning
  - `redirect.rs` — I/O redirection
  - `subshell.rs` — subshell payload types
  - `numeric.rs` — shared shell-style numeric parsing
  - `pattern.rs` — shell glob pattern matching
  - `io_context.rs` — I/O context abstraction
  - `error.rs` — ExecError types
- `src/cli/` — CLI binary (yaml_writer, error_fmt, source_map, color)
- `tests/parse.rs` + `tests/parse/` — parse tests (commands, pipelines, compound, redirects, errors, word_expansion, bash)
- `tests/exec.rs` + `tests/exec/` — execution tests (basic, expansion, arrays, printf, bash)
- `tests/cli.rs` + `tests/cli/` — CLI output tests
