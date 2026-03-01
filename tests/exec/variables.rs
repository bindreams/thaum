use crate::*;

// $- (option flags) ===================================================================================================

#[testutil::test]
fn dollar_dash_default() {
    // With no options set, $- should be a non-empty string (at minimum hB for bash defaults,
    // but our shell may start with a different set).
    let (out, _) = exec_ok("echo $-");
    // Should be non-empty and contain only flag characters
    let flags = out.trim();
    assert!(!flags.is_empty(), "$- should not be empty");
    for c in flags.chars() {
        assert!(c.is_ascii_alphabetic(), "unexpected char in $-: {:?}", c);
    }
}

#[testutil::test]
fn dollar_dash_reflects_errexit() {
    let (out, _) = exec_ok("set -e; echo $-");
    assert!(out.trim().contains('e'), "$- should contain 'e' after set -e");
}

#[testutil::test]
fn dollar_dash_reflects_nounset() {
    let (out, _) = exec_ok("set -u; echo $-");
    assert!(out.trim().contains('u'), "$- should contain 'u' after set -u");
}

#[testutil::test]
fn dollar_dash_reflects_xtrace() {
    // xtrace output goes to stderr, not stdout; just check the flag is present
    let (out, _) = exec_ok("set -x; echo $-");
    assert!(out.trim().contains('x'), "$- should contain 'x' after set -x");
}

#[testutil::test]
fn dollar_dash_not_affected_by_nounset() {
    // $- is a special parameter, so set -u should not cause an error
    let (out, _) = exec_ok("set -u; echo $-");
    assert!(!out.trim().is_empty());
}

// $_ (last argument) ==================================================================================================

#[testutil::test]
fn dollar_underscore_last_arg() {
    let (out, _) = exec_ok("echo a b c\necho $_");
    assert_eq!(out, "a b c\nc\n");
}

#[testutil::test]
fn dollar_underscore_after_single_arg_command() {
    let (out, _) = exec_ok("echo hello\necho $_");
    assert_eq!(out, "hello\nhello\n");
}

#[testutil::test]
fn dollar_underscore_after_no_arg_command() {
    // After a command with no arguments (like `true`), $_ is the command name itself
    let (out, _) = exec_ok("true\necho $_");
    assert_eq!(out, "true\n");
}
