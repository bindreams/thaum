mod common;

use common::*;
use thaum::ast::*;

fn main() {
    testutil::run_all();
}

testutil::default_labels!(lex, parse);

#[testutil::test]
fn tilde_expansion_in_assignment() {
    let cmd = first_cmd("HOME=~user");
    assert_eq!(cmd.assignments.len(), 1);
}

#[testutil::test]
fn parameter_expansion_variations() {
    let cmd = first_cmd("echo ${var:-default} ${var:=value} ${#var} ${var%suffix}");
    assert_eq!(cmd.arguments.len(), 5);
    for arg in &cmd.arguments[1..] {
        assert!(extract_word(arg)
            .parts
            .iter()
            .any(|p| matches!(p, Fragment::Parameter(ParameterExpansion::Complex { .. }))));
    }
}

#[testutil::test]
fn nested_quoting_and_expansion() {
    let cmd = first_cmd(r#"echo "hello ${name:-world}""#);
    assert_eq!(cmd.arguments.len(), 2);
    if let Fragment::DoubleQuoted(inner) = &extract_word(&cmd.arguments[1]).parts[0] {
        assert!(inner.len() >= 2);
        assert!(inner.iter().any(|p| matches!(p, Fragment::Parameter(_))));
    } else {
        panic!("expected double quoted word");
    }
}

#[testutil::test]
fn command_substitution_in_word() {
    let cmd = first_cmd("echo $(date +%Y-%m-%d)");
    assert_eq!(cmd.arguments.len(), 2);
    assert!(extract_word(&cmd.arguments[1])
        .parts
        .iter()
        .any(|p| matches!(p, Fragment::CommandSubstitution(_))));
}

#[testutil::test]
fn command_substitution_with_pipeline() {
    let cmd = first_cmd("echo $(ls | grep foo)");
    assert_eq!(cmd.arguments.len(), 2);
    if let Fragment::CommandSubstitution(stmts) = &extract_word(&cmd.arguments[1]).parts[0] {
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0].expression, Expression::Pipe { .. }));
    } else {
        panic!("expected CommandSubstitution");
    }
}

#[testutil::test]
fn arithmetic_in_echo() {
    let cmd = first_cmd("echo $((1 + 2))");
    assert_eq!(cmd.arguments.len(), 2);
    assert!(extract_word(&cmd.arguments[1])
        .parts
        .iter()
        .any(|p| matches!(p, Fragment::ArithmeticExpansion(_))));
}

#[testutil::test]
fn escaped_backtick_in_cmd_sub_double_quotes() {
    // Inside $(), a double-quoted string containing \` (escaped backtick)
    // followed by a single quote must not confuse the quoting context.
    // Source: /etc/grub.d/10_linux, 20_linux_xen, 30_os-prober use
    //   "$(gettext_printf "title \`%s' for ...")"
    let input = r#"echo "$(cmd "a \`b'c")""#;
    assert!(
        thaum::parse(input).is_ok(),
        "escaped backtick inside double quotes inside $() should not start backtick substitution"
    );
}
