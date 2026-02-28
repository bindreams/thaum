//! Conformance test cases.

use super::helpers::{assert_exit_matches_both, assert_shells_agree};
use super::preconditions;

// Exit code conformance -------------------------------------------------------

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_true() {
    assert_exit_matches_both("true");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_false() {
    assert_exit_matches_both("false");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_exit_zero() {
    assert_exit_matches_both("exit 0");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_exit_nonzero() {
    assert_exit_matches_both("exit 42");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_and_list() {
    assert_exit_matches_both("true && true");
    assert_exit_matches_both("false && true");
    assert_exit_matches_both("true && false");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_or_list() {
    assert_exit_matches_both("false || true");
    assert_exit_matches_both("true || false");
    assert_exit_matches_both("false || false");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_not() {
    assert_exit_matches_both("! true");
    assert_exit_matches_both("! false");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_variable_assignment() {
    assert_exit_matches_both("X=hello; exit 0");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_if_true() {
    assert_exit_matches_both("if true; then exit 0; else exit 1; fi");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_if_false() {
    assert_exit_matches_both("if false; then exit 0; else exit 1; fi");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_while_loop() {
    assert_exit_matches_both("X=0; while test $X != done; do X=done; done; exit 0");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_for_loop() {
    assert_exit_matches_both("for i in a b c; do true; done; exit 0");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_case_match() {
    assert_exit_matches_both("case hello in hello) exit 0;; *) exit 1;; esac");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_case_default() {
    assert_exit_matches_both("case world in hello) exit 0;; *) exit 1;; esac");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_function() {
    assert_exit_matches_both("f() { return 42; }; f; exit $?");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_test_builtin() {
    assert_exit_matches_both("test 5 -eq 5");
    assert_exit_matches_both("test 5 -eq 6");
    assert_exit_matches_both("test hello");
    assert_exit_matches_both("test ''");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_bracket_test() {
    assert_exit_matches_both("[ 3 -gt 2 ]");
    assert_exit_matches_both("[ 2 -gt 3 ]");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_parameter_default() {
    assert_exit_matches_both("X=${UNSET:-fallback}; test \"$X\" = fallback");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_break_in_loop() {
    assert_exit_matches_both("for i in a b c; do break; done; exit 0");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_multiple_statements() {
    assert_exit_matches_both("true; false; true");
}

// Stdout conformance ----------------------------------------------------------

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_shells_agree_on_echo() {
    assert_shells_agree("echo hello world");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_shells_agree_on_variable_expansion() {
    assert_shells_agree("X=hello; echo $X");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_shells_agree_on_for_loop() {
    assert_shells_agree("for i in a b c; do echo $i; done");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_shells_agree_on_if() {
    assert_shells_agree("if true; then echo yes; else echo no; fi");
}

#[testutil::test(requires = [preconditions::docker_conformance_image])]
fn conformance_shells_agree_on_case() {
    assert_shells_agree("case hello in hello) echo matched;; *) echo default;; esac");
}
