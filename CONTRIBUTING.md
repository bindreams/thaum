# Contributing

## Development setup

```sh
cargo test                    # library tests only
cargo test --features cli     # all tests including CLI output tests
cargo clippy --features cli   # lint
cargo fmt                     # format
```

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
    mod.rs             — Lexer struct, tokenization, token buffer, speculation
    char_source.rs     — CharSource: Read-backed character stream with lookahead
    operators.rs       — operator scanning (peek_at-based, no backtracking)
    word_scan.rs       — fragment scanning (literals, quotes, expansions, globs)
    heredoc.rs         — here-document body reading

  parser/
    mod.rs             — Parser struct, helpers, public parse functions
    expressions.rs     — program, lists, and-or, pipelines, leaf expressions
    commands.rs        — simple commands, redirects, heredoc helpers
    compound.rs        — if/while/for/case/brace/subshell/[[ ]]/((  ))
    bash.rs            — coproc, select, function keyword, try_parse
    helpers.rs         — keyword checks, name validation, span helpers

  word/
    mod.rs             — parse_word, parse_argument, fragment parsing
    params.rs          — parameter expansion ($var, ${var:-default}, etc.)
    subst.rs           — command substitution, arithmetic expansion

  exec/
    mod.rs             — Executor struct, main dispatch, alias expansion
    environment.rs     — Environment: variables, functions, scoping, arrays, aliases
    builtins.rs        — builtin commands (echo, cd, test, declare, printf, etc.)
    special_builtins.rs — eval, exec, source (need Executor access)
    arithmetic.rs      — arithmetic expression evaluator
    bash_test.rs       — [[ ]] conditional evaluator
    printf.rs          — printf builtin formatter
    expand.rs          — word expansion (parameters, tilde, substitution)
    compound.rs        — compound command execution (if/while/for/case)
    pipeline.rs        — pipeline execution
    external.rs        — external process spawning
    command_ex.rs      — cross-platform Command wrapper (FD mapping)
    redirect.rs        — I/O redirection handling
    subshell.rs        — subshell payload types (serialized state)
    numeric.rs         — shared shell-style numeric parsing (hex, octal, char)
    pattern.rs         — shell glob pattern matching
    io_context.rs      — I/O context abstraction (stdin/stdout/stderr)
    error.rs           — ExecError type definitions

  cli/
    mod.rs             — CLI arg parsing and dispatch
    yaml_writer.rs     — YAML AST output (YamlWriter)
    yaml_emitter.rs    — low-level YAML serialization
    yaml_value.rs      — YAML value types
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
  exec_basic.rs     — basic execution tests
  exec_conformance.rs — execution conformance tests
  corpus_runner.rs  — oils corpus test runner (custom harness)
```

## Architecture

### AST naming

| Type              | Role                                                                                  |
| ----------------- | ------------------------------------------------------------------------------------- |
| `Statement`       | Top-level wrapper: `Expression` + `ExecutionMode`. Only at list boundaries.           |
| `Expression`      | Inner command tree: `Command`, `Compound`, `FunctionDef`, `And`, `Or`, `Pipe`, `Not`. |
| `Command`         | Simple command: `arguments`, assignments, redirects.                                  |
| `Argument`        | One slot in a command's argument list: `Word(Word)` or `Atom(Atom)`.                  |
| `Word`            | Composed argument: `Vec<Fragment>` — concatenated pieces forming one shell word.      |
| `Fragment`        | Concatenable piece within a word: `Literal`, `SingleQuoted`, `Parameter`, etc.        |
| `Atom`            | Standalone argument that cannot be concatenated (e.g. `BashProcessSubstitution`).     |
| `AssignmentValue` | Right side of `=`: `Scalar(Word)` or `BashArray(Vec<Word>)`.                          |

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

The Lexer has two levels: a **character source** and a **token buffer**.

**Character source** (`CharSource`): reads characters from any `Read` source via
`BufReader`. Provides `peek()`, `peek_at(n)`, and `advance()`. Peek operations use
`RefCell` for interior mutability (logically pure — they just fill the lookahead
buffer from the reader). The character source is forward-only; there is no way to
seek backward. Operator scanning uses `peek_at(n)` for up to 3-char lookahead
instead of advance-and-restore.

**Token buffer**: a `VecDeque<SpannedToken>` that tokens are scanned into via
`scan_next()`. The parser reads tokens through `peek()`/`advance()` which pull
from this buffer. When the buffer runs out, `scan_next()` reads from the character
source and pushes one or more tokens (e.g. a newline followed by heredoc bodies).

**Speculation** (`speculate()`): saves `buf_pos`, runs a closure, and rewinds
`buf_pos` on failure. Tokens scanned during speculation stay in the buffer —
scanning state is purely cursor-side and doesn't need saving. The buffer only
shrinks from the front (on commit), never from the back.

**LastScanned** (one-token lookbehind): a `LastScanned` enum with three states —
`Fragment` (after word/fragment), `Whitespace` (after whitespace/comment), `Other`
(after operator/newline/start of input). It governs:
- **Whitespace significance**: `Whitespace` tokens are only emitted when the
  previous token was a `Fragment` (i.e. between words). Non-significant whitespace
  is consumed silently. Consecutive `Whitespace` tokens cannot exist.
- **Process substitution detection**: `<(` and `>(` are only recognized as process
  substitution when `LastScanned` is `Whitespace` (otherwise `<` is a redirect).
- **Tilde prefix recognition**: `~` at word start after whitespace or `=`.

**Key design rules**:
- The **lexer is context-free** — it produces fragment tokens (`Literal`,
  `SimpleParam`, `DoubleQuoted`, etc.), `Blank`, `IoNumber`, operators, `Newline`,
  `HereDocBody`, and `Eof`. It never promotes words to reserved word tokens.
- The **parser promotes keywords** — it checks `Token::Literal("if")` etc. when
  the grammatical context expects a keyword.
- The **lexer has no lifetime parameter** — `Lexer` (not `Lexer<'src>`). It owns
  its character source. Constructed via `Lexer::from_str()` or `Lexer::from_reader()`.
- The **parser holds the lexer directly** — `Parser { lexer: Lexer, ... }`. No
  separate TokenStream layer.
- **`speculate()`** on the Lexer uses closures for speculative parsing (e.g.,
  detecting POSIX function definitions, consuming heredoc bodies).

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
