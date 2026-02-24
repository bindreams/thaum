# thaum

A POSIX shell parser and executor written in Rust, with Bash extensions.

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
Program → Vec<Line>
Line = Vec<Statement>   (newline-delimited group)
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

Individual Bash features are controlled by `ShellOptions` flags. `Dialect::Bash` enables all of them, `Dialect::Posix` enables none. Versioned dialects (`Bash44`, `Bash50`, `Bash51`) model behavioral differences between Bash releases; `Dialect::Bash` aliases the latest (`Bash51`).

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
- `$"..."` locale translation via gettext (`locale_translation`)
- `${var@Q}`, `${var@a}`, etc. parameter transformation (`parameter_transform`)

Planned: brace expansion `{n..m}`.

## Executor

thaum includes a runtime executor that can evaluate parsed ASTs:

```rust
use thaum::exec::{Executor, CapturedIo};

let program = thaum::parse("x=hello; echo $x").unwrap();
let mut executor = Executor::new();
let mut io = CapturedIo::new();
let status = executor.execute(&program, &mut io.context()).unwrap();
assert_eq!(io.stdout_string(), "hello\n");
assert_eq!(status, 0);
```

Supported features:
- Variables (scalars, indexed arrays, associative arrays)
- Functions with local scoping
- All compound commands (if/while/until/for/case/brace groups)
- Subshells via cross-platform process spawning
- Pipelines and I/O redirection
- Alias expansion with correct newline-boundary semantics
- `[[ ]]` conditional with all operators including `=~` regex
- Arithmetic commands `(( ))` and expansion `$(( ))`
- `$"..."` locale translation via GNU gettext `.mo` catalogs
- Parameter transformation operators (`${var@Q}`, `${var@a}`, `${var@A}`, etc.)
- Indirect expansion for array keys (`${!arr[@]}`)
- Builtins: echo, printf, cd, test/[, eval, exec, source, declare/typeset,
  export, unset, read, set, shopt, alias/unalias, local, readonly, shift,
  return, break, continue, exit, true, false, :

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
