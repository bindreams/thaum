mod common;

use common::*;
use thaum::ast::*;

#[test]
fn simple_ls() {
    let cmd = first_cmd("ls -la /tmp");
    assert_eq!(cmd.arguments.len(), 3);
    assert_eq!(word_literal(&cmd.arguments[0]), "ls");
    assert_eq!(word_literal(&cmd.arguments[1]), "-la");
    assert_eq!(word_literal(&cmd.arguments[2]), "/tmp");
}

#[test]
fn echo_with_variable() {
    let cmd = first_cmd("echo $HOME");
    assert_eq!(cmd.arguments.len(), 2);
    assert!(extract_word(&cmd.arguments[1])
        .parts
        .iter()
        .any(|p| matches!(p, Fragment::Parameter(_))));
}

#[test]
fn command_with_quoted_args() {
    let cmd = first_cmd(r#"echo "hello world" 'single quoted'"#);
    assert_eq!(cmd.arguments.len(), 3);
    assert!(extract_word(&cmd.arguments[1])
        .parts
        .iter()
        .any(|p| matches!(p, Fragment::DoubleQuoted(_))));
    assert!(extract_word(&cmd.arguments[2])
        .parts
        .iter()
        .any(|p| matches!(p, Fragment::SingleQuoted(_))));
}

#[test]
fn assignment_with_expansion() {
    let cmd = first_cmd("PATH=$HOME/bin:$PATH");
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.assignments[0].name, "PATH");
}

#[test]
fn multiple_assignments_then_command() {
    let cmd = first_cmd("CC=gcc CFLAGS=-O2 make");
    assert_eq!(cmd.assignments.len(), 2);
    assert_eq!(cmd.arguments.len(), 1);
    assert_eq!(word_literal(&cmd.arguments[0]), "make");
}

#[test]
fn reserved_words_as_arguments() {
    let cmd = first_cmd("echo if then else fi do done while until for case esac in");
    assert_eq!(cmd.arguments.len(), 13);
}

#[test]
fn background_and_sequential() {
    let prog = parse_ok("cmd1 & cmd2; cmd3 &");
    assert_eq!(prog.statements.len(), 3);
    assert_eq!(prog.statements[0].mode, ExecutionMode::Background);
    assert_eq!(prog.statements[1].mode, ExecutionMode::Terminated);
    assert_eq!(prog.statements[2].mode, ExecutionMode::Background);
}

#[test]
fn semicolon_separated_commands_on_one_line() {
    let prog = parse_ok("echo a; echo b; echo c");
    assert_eq!(prog.statements.len(), 3);
}

#[test]
fn script_with_multiple_commands() {
    let input = r#"#!/bin/sh
echo "Starting..."
cd /tmp
ls -la
echo "Done""#;
    let prog = parse_ok(input);
    assert!(prog.statements.len() >= 3);
}

#[test]
fn empty_program() {
    assert!(parse_ok("").statements.is_empty());
}

#[test]
fn whitespace_only_program() {
    assert!(parse_ok("   \n\n  \n").statements.is_empty());
}

#[test]
fn comment_only() {
    assert!(parse_ok("# this is a comment").statements.is_empty());
}

#[test]
fn comment_after_command() {
    let prog = parse_ok("echo hello # comment\necho world");
    assert_eq!(prog.statements.len(), 2);
}

#[test]
fn posix_function_definition() {
    let e = first_expr("myfunc() { echo hello; }");
    if let Expression::FunctionDef(f) = &e {
        assert_eq!(f.name, "myfunc");
        assert!(matches!(
            f.body.as_ref(),
            CompoundCommand::BraceGroup { .. }
        ));
    } else {
        panic!("expected FunctionDef, got {:?}", e);
    }
}

#[test]
fn negated_command() {
    assert!(matches!(first_expr("! cmd"), Expression::Not(_)));
}

#[test]
fn negated_pipeline() {
    if let Expression::Not(inner) = &first_expr("! cmd1 | cmd2") {
        assert!(matches!(inner.as_ref(), Expression::Pipe { .. }));
    } else {
        panic!("expected Not");
    }
}

#[test]
fn assignment_with_quoted_value() {
    let cmd = first_cmd(r#"FOO="hello $USER""#);
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.assignments[0].name, "FOO");
}
