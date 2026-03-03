# testutil

Runtime test preconditions with unavailability reporting for Rust.

Rust's built-in test framework has no way to mark a test as "ignored with reason" at runtime. Tests that need external tools (valgrind, docker, a built binary) either silently pass when the tool is missing, or hard-fail.

`testutil` solves this with `#[requires(...)]` — an attribute that declares runtime preconditions on test functions. Unmet preconditions produce an `ignored` status and a summary showing exactly what's missing.

## Setup

Add a `[[test]]` target with `harness = false` in your `Cargo.toml`:

```toml
[dev-dependencies]
testutil = { path = "testutil" }

[[test]]
name = "my_tests"
path = "tests/my_tests.rs"
harness = false
```

Create the test entry point:

```rust
// tests/my_tests.rs
#[path = "my_tests_support/mod.rs"]
mod support;

fn main() {
    testutil::run_all();
}
```

## Writing tests

Annotate test functions with `#[requires(...)]`. Each argument must be a function
`fn() -> Result<(), String>` — returning `Ok(())` means the requirement is met,
`Err(reason)` means it's not.

```rust
use testutil::requires;

fn valgrind() -> Result<(), String> {
    testutil::probe_executable("valgrind")
}

fn my_binary() -> Result<(), String> {
    testutil::probe_path("target/debug/my_binary")
}

#[requires(valgrind, my_binary)]
fn smoke_test() {
    // This body only runs if both valgrind and my_binary are available.
}
```

Tests with no `#[requires]` attribute are not registered by this harness. Use
`#[requires()]` (empty argument list) for tests with no preconditions that should
still be collected by the harness.

## Built-in probe helpers

| Function                 | Checks                      |
| ------------------------ | --------------------------- |
| `probe_executable(name)` | `<name> --version` succeeds |
| `probe_path(path)`       | File or directory exists    |

## Output

When all requirements are met:

```
running 2 tests
test smoke_test     ... ok
test full_pipeline  ... ok

test result: ok. 2 passed; 0 failed; 0 ignored
```

When a requirement is missing:

```
running 2 tests
test smoke_test     ... ignored
test full_pipeline  ... ignored

test result: ok. 0 passed; 0 failed; 2 ignored

--- Unavailable (2) ---
  smoke_test:     valgrind not installed
  full_pipeline:  valgrind not installed
```

## How it works

1. `#[requires(...)]` is a proc macro that preserves the original function and
   appends an `inventory::submit!` call to register it with the harness.
1. `run_all()` iterates all registered tests, checks preconditions at runtime,
   and builds `libtest-mimic::Trial`s — marking unmet tests as ignored.
1. After `libtest-mimic::run()` completes, the unavailability summary is printed
   to stderr.
