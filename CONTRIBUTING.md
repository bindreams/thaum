# Contributing

## Development setup

```sh
cargo test                    # library tests only
cargo test --features cli     # all tests including CLI output tests
cargo clippy --features cli   # lint
cargo fmt                     # format
```

## Test-driven development

Write tests before or alongside implementation. When adding a feature:

1. Write a failing test that describes the expected behavior
2. Implement until the test passes
3. Run `cargo clippy` and `cargo fmt`

Do not verify things by manually running them — write tests instead.

## Project structure

```
src/
  lib.rs               — public API: parse(), parse_with()
  main.rs              — CLI entry point (thin dispatch)
  ast.rs               — AST types: Program, Statement, Expression, Command, ...
  token.rs             — Token enum with doc comments
  dialect.rs           — ParseOptions, Dialect (Posix/Bash)
  span.rs              — byte offset spans for source locations
  error.rs             — LexError, ParseError with spans

  lexer/
    mod.rs             — Lexer struct, tokenization, operators, word scanning
    cursor.rs          — low-level character cursor over source text
    heredoc.rs         — here-document reading

  parser/
    mod.rs             — Parser struct, helpers, public parse functions
    token_stream.rs    — TokenStream: buffered peek/advance with checkpoint/rewind
    expressions.rs     — program, lists, and-or, pipelines, leaf expressions
    commands.rs        — simple commands, redirects, heredoc helpers
    compound.rs        — if/while/for/case/brace/subshell/[[ ]]/((  ))
    bash.rs            — coproc, select, function keyword, try_parse
    helpers.rs         — keyword checks, name validation, span helpers

  word/
    mod.rs             — parse_word, parse_argument, fragment parsing
    params.rs          — parameter expansion ($var, ${var:-default}, etc.)
    subst.rs           — command substitution, arithmetic expansion

  cli/
    mod.rs             — CLI arg parsing and dispatch
    yaml_writer.rs     — YAML AST output (YamlWriter)
    error_fmt.rs       — compiler-style error display
    source_map.rs      — byte offset → line:col mapping
    color.rs           — colored terminal output
tests/
  common/mod.rs     — shared test helpers (parse_ok, first_cmd, etc.)
  commands.rs       — simple commands, assignments, execution modes
  pipelines.rs      — pipelines, and-or chains, operator precedence
  compound.rs       — if/while/for/case/brace/subshell
  redirects.rs      — redirections, here-documents
  errors.rs         — error handling (unclosed, stray operators, etc.)
  word_expansion.rs — parameter expansion, command substitution, globs
  bash_features.rs  — all Bash extensions with POSIX rejection tests
  cli_output.rs     — CLI output format regression tests (requires --features cli)
```

## Architecture

### AST naming

| Type | Role |
|------|------|
| `Statement` | Top-level wrapper: `Expression` + `ExecutionMode`. Only at list boundaries. |
| `Expression` | Inner command tree: `Command`, `Compound`, `FunctionDef`, `And`, `Or`, `Pipe`, `Not`. |
| `Command` | Simple command: `arguments`, assignments, redirects. |
| `Argument` | One slot in a command's argument list: `Word(Word)` or `Atom(Atom)`. |
| `Word` | Composed argument: `Vec<Fragment>` — concatenated pieces forming one shell word. |
| `Fragment` | Concatenable piece within a word: `Literal`, `SingleQuoted`, `Parameter`, etc. |
| `Atom` | Standalone argument that cannot be concatenated (e.g. `BashProcessSubstitution`). |
| `AssignmentValue` | Right side of `=`: `Scalar(Word)` or `BashArray(Vec<Word>)`. |

`ExecutionMode` has three variants:
- `Sequential` — newline-terminated or last in list
- `Terminated` — explicitly terminated with `;` (semantically distinct from newline for `set -e`)
- `Background` — followed by `&`

### Operator precedence

From lowest to highest:
1. `&&` / `||` — builds `And`/`Or` nodes (left-associative)
2. `!` — wraps in `Not` (applies to entire pipe chain)
3. `|` — builds `Pipe` nodes (left-associative)

### Lexer/Parser architecture

- The **lexer is context-free** — it produces only `Word`, `IoNumber`, operators, `Newline`, and `Eof`. It never promotes words to reserved word tokens.
- The **parser promotes keywords** — it checks `Token::Word("if")` etc. when the grammatical context expects a keyword.
- **TokenStream** sits between lexer and parser: buffers tokens, provides `peek()`/`advance()`, and supports `checkpoint()`/`rewind()`/`release()` for speculative parsing.
- **`try_parse`** on the Parser uses closures for speculative parsing (e.g., detecting POSIX function definitions).

### Dialect system

- `ParseOptions` has boolean flags for individual Bash features
- `Dialect::Posix` = all false, `Dialect::Bash` = all true
- The lexer uses `ParseOptions` to conditionally recognize Bash-specific operator tokens (e.g., `<<<`, `&>`)
- The parser checks `ParseOptions` before accepting Bash keywords (`function`, `select`, `coproc`)
- In tests, enable only the specific flag being tested

### Adding a new Bash feature

1. Add a flag to `ParseOptions` in `dialect.rs`
2. Set it to `true` in `Dialect::Bash`
3. If the feature needs new tokens, add them to `token.rs` with `Bash` prefix (e.g. `BashHereStringOp`) and `display_name()`
4. Update the lexer in `lexer/mod.rs` to recognize them conditionally on the flag
5. Add AST types to `ast.rs` if needed — use `Bash` prefix on Bash-specific variants (e.g. `BashDoubleBracket`, `BashProcessSubstitution`)
6. Standalone argument types go in `Atom`, concatenable pieces in `Fragment`, assignment-only types in `AssignmentValue`
7. Update the parser (likely `parser/compound.rs` or `parser/bash.rs`)
8. Update the CLI emitter in `cli/yaml_writer.rs`
9. Write tests with a `ParseOptions` that only enables the new flag
10. Write a test that POSIX mode rejects the new syntax

### Error messages

- Use `Token::display_name()` for human-readable names in errors — never `{:?}`
- Errors carry `Span` for source location
- The CLI displays compiler-style errors with source context and `^^^` underlines

### Command substitutions

`$(...)` content is recursively parsed into `Vec<Statement>`. Source spans inside are relative to the substitution substring, not the original source. The CLI skips source annotations inside substitutions to avoid misleading locations.
