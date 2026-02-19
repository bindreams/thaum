# thaum

A POSIX shell syntax parser written in Rust, with optional Bash extensions.

## Library

```rust
use thaum::{parse, parse_with, Dialect};

// POSIX mode (default)
let program = parse("echo hello | grep h").unwrap();

// Bash mode (enables <<<, &>, [[ ]], etc.)
let program = parse_with("cat <<< hello", Dialect::Bash).unwrap();
```

### AST structure

```
Program → Vec<Statement>
Statement = Expression + ExecutionMode (Sequential | Terminated | Background)
Expression = Command | Compound | FunctionDef | And | Or | Pipe | Not

Command    — simple command: arguments, assignments, redirects
Compound   — if/while/until/for/case/brace group/subshell
And/Or     — left && right, left || right (binary tree, left-associative)
Pipe       — left | right (binary tree, left-associative)
Not        — ! expression

Argument   — Word(Word) | Atom(Atom)
Word       — composed argument: Vec<Fragment> (concatenated pieces)
Fragment   — Literal | SingleQuoted | DoubleQuoted | Parameter | CommandSubstitution | ...
Atom       — standalone argument (e.g. process substitution <(cmd))
```

Key types: `Program`, `Statement`, `Expression`, `Command`, `CompoundCommand`, `Argument`, `Word`, `Fragment`, `Atom`, `Redirect`, `AssignmentValue`.

### Dialect system

Individual Bash features are controlled by `ParseOptions` flags. `Dialect::Bash` enables all of them, `Dialect::Posix` enables none.

Currently implemented Bash extensions:
- `<<<` here-strings (`here_strings`)
- `&>` / `&>>` redirects (`ampersand_redirect`)
- `[[ ]]` extended test (`double_brackets`)
- `(( ))` arithmetic command (`arithmetic_command`)
- `<()` / `>()` process substitution (`process_substitution`)
- `;&` / `;;&` extended case (`extended_case`)
- `var=(...)` arrays (`arrays`)
- `coproc` command (`coproc`)
- `select` loop (`select`)
- `function` keyword (`function_keyword`)

Planned: brace expansion `{n..m}`, `=~` regex match.

## CLI tool

```sh
# Build
cargo build --features cli

# Parse and dump AST as YAML
echo 'echo hello | grep h' | cargo run --features cli -- -
cargo run --features cli -- script.sh

# Bash mode
cargo run --features cli -- --bash script.sh

# Errors are displayed with source context
echo 'if true; then fi' | cargo run --features cli -- -
# error: unexpected token 'fi', expected a command in then body
#  --> <stdin>:1:15
#   |
# 1 | if true; then fi
#   |               ^^
```
