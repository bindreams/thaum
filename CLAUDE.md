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

Place contracts on every function where there is a meaningful invariant to check. Contracts are documentation that also catches regressions.

## Architecture

### Lexer/Parser pipeline
- The **lexer is context-free** — it produces `Word`, `IoNumber`, operators, `Newline`, `HereDocBody`, and `Eof`. It never promotes words to reserved word tokens.
- The **parser promotes keywords** — it matches `Token::Word("if")` etc. when the grammatical context expects a keyword.
- **TokenStream** sits between lexer and parser: buffers tokens, provides `peek()`/`advance()`, and supports `checkpoint()`/`rewind()`/`release()` for speculative parsing.
- **`try_parse`** on the Parser uses closures for speculative parsing (e.g., detecting POSIX function definitions).
- The lexer handles heredocs autonomously — no parser→lexer feedback.

### AST naming conventions
- **Statement** — top-level wrapper with `ExecutionMode` (Sequential/Terminated/Background). Only appears at list boundaries.
- **Expression** — the command tree enum: `Command`, `Compound`, `FunctionDef`, `And`, `Or`, `Pipe`, `Not`.
- **Argument** — `Word(Word)` | `Atom(Atom)`. One slot in a command's argument list.
- **Word** — `Vec<Fragment>`. Concatenable pieces forming one shell word.
- **Fragment** — Literal, SingleQuoted, DoubleQuoted, Parameter, CommandSubstitution, etc.
- **Atom** — standalone argument (BashProcessSubstitution). Cannot be concatenated.
- **AssignmentValue** — `Scalar(Word)` | `BashArray(Vec<Word>)`.

### Dialect system
- `ParseOptions` has individual boolean flags for each Bash feature
- `Dialect::Posix` = all false, `Dialect::Bash` = all true
- The lexer conditionally recognizes Bash operator tokens based on options
- The parser checks options before accepting Bash keywords
- In tests, enable only the specific flag being tested

### Token naming
- Bash-specific tokens use `Bash` prefix: `BashHereStringOp`, `BashDblLBracket`, etc.
- Semantic names, not character mnemonics: `RedirectFromFile` not `Less`
- POSIX spec names in doc comments: `/// '<' — redirect input (POSIX: LESS)`

### Key design decisions
- `&&`, `||`, `|` are binary tree nodes in `Expression`, not flat lists
- `!` wraps the entire pipe chain: `! a | b` → `Not(Pipe(a, b))`
- Precedence (low→high): `&&`/`||`, `!`, `|`
- `;` and `&` only exist on `Statement`, never on inner `Expression` nodes
- `ExecutionMode::Terminated` (`;`) vs `Sequential` (newline) is semantically significant for `set -e`
- Command substitutions (`$(...)`) are recursively parsed into `Vec<Statement>`
- Source spans inside command substitutions are relative to the substring

### Project structure
- `src/lexer/` — context-free tokenizer (cursor, heredoc, operators, words)
- `src/parser/` — recursive descent (token_stream, expressions, commands, compound, bash, helpers)
- `src/word/` — word expansion parsing (fragments, params, substitution)
- `src/cli/` — CLI binary (yaml_writer, error_fmt, source_map, color)
- `tests/` — split by topic: commands, pipelines, compound, redirects, errors, word_expansion, bash_features, cli_output
