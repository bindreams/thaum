# Contributing

## Development setup

```sh
pip install pre-commit && pre-commit install   # one-time setup
cargo nextest run --features cli,testkit               # all tests (excludes conformance)
cargo nextest run -P conformance --features cli,testkit # conformance tests (requires Docker image)
cargo test --features cli,testkit                      # also works (same harness)
pre-commit run --all-files                     # lint + format + #[test] guard
```

### Test macro

Use `#[skuld::test]` instead of `#[test]`. All test binaries use `harness = false`
with the `skuld` custom harness. A bare `#[test]` compiles but silently never runs.

```rust
#[skuld::test]                                     // simple test
#[skuld::test(requires = [preconditions::docker])]  // runtime precondition
#[skuld::test(labels = [slow])]                     // label for filtering
```

For dynamic tests (e.g. corpus YAML files), use `skuld::TestRunner::add()`.

### Benchmarks

Benchmarks (requires Docker, or `valgrind`/`hyperfine` for `--no-sandbox`):

```sh
cargo bench --bench bench                                         # Docker (default)
cargo bench --bench bench -- --no-sandbox --kind instructions     # local callgrind
cargo bench --bench bench -- --no-sandbox --kind walltime         # local hyperfine
```

## Project structure

```
src/
  lib.rs               — public API: parse(), parse_with()
  main.rs              — CLI entry point (thin dispatch)
  ast.rs               — AST types: Program, Statement, Expression, Command, ...
  token.rs             — Token enum with doc comments
  dialect.rs           — ShellOptions, Dialect (Posix/Bash44/Bash50/Bash51/Bash)
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
    gettext.rs         — GNU gettext catalog lookup for $"..." locale translation
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
  common/           — shared test helpers (parse_ok, first_cmd, docker)
  parse.rs + parse/ — parse tests (commands, pipelines, compound, redirects, errors, word_expansion, bash)
  exec.rs + exec/   — execution tests (basic, expansion, arrays, printf, bash)
  cli.rs + cli/     — CLI output format regression tests (requires --features cli,testkit)
  corpus.rs         — oils corpus test runner (custom harness)
benches/
  bench.rs        — unified benchmark binary (callgrind + hyperfine backends)
  bench/          — backend modules (callgrind, hyperfine, types, format, docker)
  scripts/        — benchmark scripts (.sh.yaml format, auto-discovered by build.rs)
  docker/         — Dockerfile for sandboxed benchmark execution
```

## Documentation style

Follow [Rust API Guidelines C-CRATE-DOC](https://rust-lang.github.io/api-guidelines/documentation.html)
and [the rustdoc book](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html).

### Comment types

| Syntax | Scope                      | Use for                                           |
| ------ | -------------------------- | ------------------------------------------------- |
| `//!`  | Parent item (module/crate) | File-level docs at the top of every `.rs` file    |
| `///`  | Next item                  | Public functions, structs, enums, traits, methods |
| `//`   | None (not rendered)        | Implementation notes, TODOs, non-doc remarks      |

### File-level docs (`//!`)

Every `.rs` file starts with a `//!` block describing the module's purpose.
Place it before any `use` statements. One sentence is enough for small modules;
larger ones benefit from a paragraph and cross-references.

```rust
//! Shell-style glob pattern matching.
//!
//! Supports `*` (any chars), `?` (single char), and `[...]` (char class).
//! Used by `case` pattern matching and `[[ == ]]` string comparison.

use crate::...;
```

### Item docs (`///`)

All public items (`pub fn`, `pub struct`, `pub enum`, `pub trait`, `pub type`)
must have a `///` doc comment. Internal items should have them when the logic
is non-obvious.

The **first paragraph** (before the first blank line) is the summary. It shows
in module indexes and search results. Keep it to **one sentence** in third-person
present tense:

```rust
/// Returns the scalar string value of this variable.
///
/// For indexed arrays, returns element 0 (bash: `$a` == `${a[0]}`).
/// For associative arrays, returns element with key `"0"`, or `""`.
pub fn scalar_str(&self) -> &str { ... }
```

### Standard sections

Use these headings when applicable (order: summary, details, sections, examples):

| Heading      | When to include                                                   |
| ------------ | ----------------------------------------------------------------- |
| `# Panics`   | The function can panic under certain conditions                   |
| `# Errors`   | The function returns `Result` — list error variants               |
| `# Safety`   | The function is `unsafe` — list invariants the caller must uphold |
| `# Examples` | Always on public items; encouraged on complex internal items      |

### Cross-references

Link to other types with backtick-bracket syntax. rustdoc resolves them
automatically:

```rust
/// Expands a [`Word`] into a string, handling [`ParameterExpansion`]
/// and [`CommandSubstitution`] fragments.
```

### What NOT to document

- Don't restate the type signature (`/// Takes a &str and returns an i64`).
- Don't add `///` to trivially-named enum variants where the variant name says it all.
- Don't use doc comments for implementation notes — use `//` instead.

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
1. `!` — wraps in `Not` (applies to entire pipe chain)
1. `|` — builds `Pipe` nodes (left-associative)

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

- `ShellOptions` has boolean flags for individual Bash features
- `Dialect::Posix` = all false, `Dialect::Bash` = all true
- Versioned dialects (`Bash44`, `Bash50`, `Bash51`) model behavioral differences between Bash releases; `Dialect::Bash` aliases the latest (`Bash51`)
- The lexer uses `ShellOptions` to conditionally recognize Bash-specific operator tokens (e.g., `<<<`, `&>`)
- The parser checks `ShellOptions` before accepting Bash keywords (`function`, `select`, `coproc`)
- In tests, enable only the specific flag being tested

### Adding a new Bash feature

1. Add a flag to `ShellOptions` in `dialect.rs`
1. Set it to `true` in the appropriate `Dialect` variants (and `Bash51`/`Bash` at minimum)
1. If the feature needs new tokens, add them to `token.rs` with `Bash` prefix (e.g. `BashHereStringOp`) and `display_name()`
1. Update the lexer in `lexer/mod.rs` to recognize them conditionally on the flag
1. Add AST types to `ast.rs` if needed — use `Bash` prefix on Bash-specific variants (e.g. `BashDoubleBracket`, `BashProcessSubstitution`)
1. Standalone argument types go in `Atom`, concatenable pieces in `Fragment`, assignment-only types in `AssignmentValue`
1. Update the parser (likely `parser/compound.rs` or `parser/bash.rs`)
1. Update the CLI emitter in `cli/yaml_writer.rs`
1. Write tests with a `ShellOptions` that only enables the new flag
1. Write a test that POSIX mode rejects the new syntax

### Error messages

- Use `Token::display_name()` for human-readable names in errors — never `{:?}`
- Errors carry `Span` for source location
- The CLI displays compiler-style errors with source context and `^^^` underlines

### Command substitutions

`$(...)` content is recursively parsed into `Vec<Statement>`. Source spans inside are relative to the substitution substring, not the original source. The CLI skips source annotations inside substitutions to avoid misleading locations.

### Alias expansion

Aliases are expanded at execution time, not parse time. The executor substitutes
the alias value and re-parses the resulting command line. This design is correct
for all real-world alias patterns (see taxonomy below) but cannot handle aliases
whose expansion changes the grammatical structure of surrounding code.

#### Alias Funkiness Taxonomy

| Level                        | Description                      | Example                                       | Parseable without expansion?          |
| ---------------------------- | -------------------------------- | --------------------------------------------- | ------------------------------------- |
| **1. Single word**           | Command rename                   | `alias g="git"`                               | Yes (identical AST shape)             |
| **2. Multiple words**        | Command + flags/args             | `alias ll="ls -lah"`                          | Yes (extra arg nodes)                 |
| **3a. Redirections**         | Adds I/O redirects               | `alias quiet="cmd 2>/dev/null"`               | Yes (slightly different AST)          |
| **3b. Command substitution** | `$(...)` in value                | `alias gcm="git checkout $(git_main_branch)"` | Yes (substitution is inside a word)   |
| **3c. Trailing space**       | Triggers chained alias expansion | `alias sudo="sudo "`                          | Yes (just a word with trailing space) |
| **4. Separators**            | Contains `;`, `\|`, `&&`, `\|\|` | `alias gwip="git add -A; git rm ..."`         | Yes (wrong AST, but parses)           |
| **5. Partial compound**      | Unbalanced keywords/braces       | `alias LEFT="{"`                              | **No** (parse error)                  |

Levels 1–4 are fully supported. Level 5 is **not supported** and will not be: a
survey of ~2,500 real-world aliases found 0% at level 5. Both POSIX and Bash allow
it in theory, but in practice nobody writes `alias LEFT='{'` — functions are used
instead. Supporting level 5 would require moving alias expansion into the parser
(feeding expanded text back into the token stream), a significant architectural
change with no practical benefit.
