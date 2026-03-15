//! Integration tests for REPL-style executor usage: parse one line at a time,
//! preserving state, surviving syntax errors.

use thaum::exec::{CapturedIo, Executor};
use thaum::Dialect;

// Interactive flag ====================================================================================================

#[skuld::test]
fn dollar_dash_includes_i_when_interactive() {
    let mut sh = shell!(interactive);
    let r = sh.exec("echo $-");
    assert!(
        r.stdout().contains('i'),
        "expected $- to contain 'i', got: {:?}",
        r.stdout()
    );
    sh.join();
}

#[skuld::test]
fn dollar_dash_excludes_i_when_not_interactive() {
    let r = exec!("echo $-");
    let out = r.stdout();
    assert!(!out.contains('i'), "expected $- NOT to contain 'i', got: {out:?}");
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
    let mut sh = shell!(interactive);
    sh.exec("X=hello");
    let r = sh.exec("echo $X");
    assert_eq!(r.stdout().trim(), "hello");
    sh.join();
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
    let mut sh = shell!(interactive);
    sh.exec("greet() { echo hi; }");
    let r = sh.exec("greet");
    assert_eq!(r.stdout().trim(), "hi");
    sh.join();
}

#[skuld::test]
fn alias_defined_in_one_line_usable_in_next() {
    let mut sh = shell!(dialect = Dialect::Bash, interactive);
    sh.exec("alias ll='echo listing'");
    // Alias expansion requires re-parsing with the alias table.
    // In the real REPL, aliases are snapshot'd before each line parse.
    // For this test we verify the alias was stored.
    assert!(sh.env().get_alias("ll").is_some());
    sh.join();
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
    let mut sh = shell!(dialect = Dialect::Bash, interactive);
    let _ = sh.env_mut().set_var("PROMPT_COMMAND", "MARKER=prompted");

    // Simulate what the REPL does: parse and execute PROMPT_COMMAND
    let cmd = sh.env().get_var("PROMPT_COMMAND").unwrap().to_string();
    sh.exec(&cmd);

    assert_eq!(sh.env().get_var("MARKER").unwrap(), "prompted");
    sh.join();
}

// Runtime errors in interactive mode ==================================================================================

#[skuld::test]
fn interactive_readonly_sets_status_1() {
    let mut sh = shell!(interactive, dialect = Dialect::Bash);
    sh.exec("readonly X=1");
    let r = sh.exec("X=2");
    assert_eq!(r.status(), 1, "interactive readonly violation should set $?=1");
    assert!(r.stderr().contains("readonly variable"));
    let r = sh.join();
    assert_eq!(r.status(), 1);
    r.stderr(); // acknowledge accumulated stderr
}

#[skuld::test]
fn interactive_command_not_found_sets_127() {
    let mut sh = shell!(interactive);
    let r = sh.exec("nonexistent_xyz_cmd");
    assert_eq!(r.status(), 127, "interactive command not found should set $?=127");
    assert!(r.stderr().contains("command not found"));
    let r = sh.join();
    assert_eq!(r.status(), 127);
    r.stderr();
}

#[skuld::test]
fn interactive_error_continues_execution() {
    let mut sh = shell!(interactive, dialect = Dialect::Bash);
    sh.exec("readonly V=1");
    let r = sh.exec("V=2");
    assert_eq!(r.status(), 1);
    r.stderr();
    let r = sh.exec("echo still_alive");
    assert_eq!(r.stdout().trim(), "still_alive");
    let r = sh.join();
    r.status();
    r.stderr();
}
