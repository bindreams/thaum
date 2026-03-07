//! Integration tests for REPL-style executor usage: parse one line at a time,
//! preserving state, surviving syntax errors.

use thaum::exec::{CapturedIo, Executor};
use thaum::Dialect;

// Interactive flag ====================================================================================================

#[skuld::test]
fn dollar_dash_includes_i_when_interactive() {
    let program = thaum::parse("echo $-").unwrap();
    let mut exec = Executor::new();
    exec.env_mut().set_interactive(true);
    let mut io = CapturedIo::new();
    let _ = exec.execute(&program, &mut io.context());
    let output = io.stdout_string();
    assert!(output.contains('i'), "expected $- to contain 'i', got: {output:?}");
}

#[skuld::test]
fn dollar_dash_excludes_i_when_not_interactive() {
    let program = thaum::parse("echo $-").unwrap();
    let mut exec = Executor::new();
    let mut io = CapturedIo::new();
    let _ = exec.execute(&program, &mut io.context());
    let output = io.stdout_string();
    assert!(!output.contains('i'), "expected $- NOT to contain 'i', got: {output:?}");
}

#[skuld::test]
fn expand_aliases_on_by_default_when_interactive() {
    let mut exec = Executor::new();
    assert!(!exec.env().expand_aliases_enabled());
    exec.env_mut().set_interactive(true);
    assert!(exec.env().expand_aliases_enabled());
}

// REPL-style state persistence ========================================================================================

#[skuld::test]
fn state_persists_across_lines() {
    let mut exec = crate::test_executor();
    exec.env_mut().set_interactive(true);
    let mut io = CapturedIo::new();

    // Line 1: set a variable
    let prog1 = thaum::parse("X=hello").unwrap();
    let _ = exec.execute(&prog1, &mut io.context());

    // Line 2: use the variable
    let prog2 = thaum::parse("echo $X").unwrap();
    let _ = exec.execute(&prog2, &mut io.context());

    assert_eq!(io.stdout_string().trim(), "hello");
}

#[skuld::test]
fn syntax_error_does_not_poison_executor() {
    let mut exec = crate::test_executor();
    exec.env_mut().set_interactive(true);

    // Line 1: syntax error
    let err = thaum::parse("if");
    assert!(err.is_err(), "expected parse error for 'if'");

    // Line 2: valid command still works
    let prog = thaum::parse("echo ok").unwrap();
    let mut io = CapturedIo::new();
    let _ = exec.execute(&prog, &mut io.context());
    assert_eq!(io.stdout_string().trim(), "ok");
}

#[skuld::test]
fn function_defined_in_one_line_callable_in_next() {
    let mut exec = crate::test_executor();
    exec.env_mut().set_interactive(true);

    let prog1 = thaum::parse("greet() { echo hi; }").unwrap();
    let mut io = CapturedIo::new();
    let _ = exec.execute(&prog1, &mut io.context());

    let prog2 = thaum::parse("greet").unwrap();
    let _ = exec.execute(&prog2, &mut io.context());
    assert_eq!(io.stdout_string().trim(), "hi");
}

#[skuld::test]
fn alias_defined_in_one_line_usable_in_next() {
    let mut exec = crate::test_bash_executor();
    exec.env_mut().set_interactive(true);

    let prog1 = thaum::parse_with("alias ll='echo listing'", Dialect::Bash).unwrap();
    let mut io = CapturedIo::new();
    let _ = exec.execute(&prog1, &mut io.context());

    // Alias expansion requires re-parsing with the alias table
    // In the real REPL, aliases are snapshot'd before each line parse.
    // For this test we verify the alias was stored.
    assert!(exec.env().get_alias("ll").is_some());
}

// PS1/PS2 defaults ====================================================================================================

#[skuld::test]
fn posix_interactive_defaults_ps1() {
    let options = Dialect::Posix.options();
    let mut exec = Executor::with_options(options.clone());
    exec.env_mut().set_interactive(true);
    exec.env_mut().set_interactive_defaults(&options);
    let ps1 = exec.env().get_var("PS1").unwrap();
    // POSIX: "$ " for non-root, "# " for root
    assert!(ps1 == "$ " || ps1 == "# ");
}

#[skuld::test]
fn bash_interactive_defaults_ps1() {
    let options = Dialect::Bash.options();
    let mut exec = Executor::with_options(options.clone());
    exec.env_mut().set_interactive(true);
    exec.env_mut().set_interactive_defaults(&options);
    assert_eq!(exec.env().get_var("PS1").unwrap(), r"\s-\v\$ ");
}

#[skuld::test]
fn interactive_defaults_ps2() {
    let options = Dialect::Posix.options();
    let mut exec = Executor::with_options(options.clone());
    exec.env_mut().set_interactive(true);
    exec.env_mut().set_interactive_defaults(&options);
    assert_eq!(exec.env().get_var("PS2").unwrap(), "> ");
}

#[skuld::test]
fn interactive_defaults_ps4() {
    let options = Dialect::Posix.options();
    let mut exec = Executor::with_options(options.clone());
    exec.env_mut().set_interactive(true);
    exec.env_mut().set_interactive_defaults(&options);
    assert_eq!(exec.env().get_var("PS4").unwrap(), "+ ");
}

// PROMPT_COMMAND ======================================================================================================

#[skuld::test]
fn prompt_command_sets_variable() {
    let mut exec = crate::test_bash_executor();
    exec.env_mut().set_interactive(true);
    let _ = exec.env_mut().set_var("PROMPT_COMMAND", "MARKER=prompted");

    // Simulate what the REPL does: parse and execute PROMPT_COMMAND
    let cmd = exec.env().get_var("PROMPT_COMMAND").unwrap().to_string();
    let prog = thaum::parse(&cmd).unwrap();
    let mut io = CapturedIo::new();
    let _ = exec.execute(&prog, &mut io.context());

    assert_eq!(exec.env().get_var("MARKER").unwrap(), "prompted");
}
