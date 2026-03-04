use crate::common::*;
use thaum::ast::*;

#[skuld::test]
fn simple_ls() {
    let cmd = first_cmd("ls -la /tmp");
    assert_eq!(cmd.arguments.len(), 3);
    assert_eq!(word_literal(&cmd.arguments[0]), "ls");
    assert_eq!(word_literal(&cmd.arguments[1]), "-la");
    assert_eq!(word_literal(&cmd.arguments[2]), "/tmp");
}

#[skuld::test]
fn echo_with_variable() {
    let cmd = first_cmd("echo $HOME");
    assert_eq!(cmd.arguments.len(), 2);
    assert!(extract_word(&cmd.arguments[1])
        .parts
        .iter()
        .any(|p| matches!(p, Fragment::Parameter(_))));
}

#[skuld::test]
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

#[skuld::test]
fn assignment_with_expansion() {
    let cmd = first_cmd("PATH=$HOME/bin:$PATH");
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.assignments[0].name, "PATH");
}

#[skuld::test]
fn multiple_assignments_then_command() {
    let cmd = first_cmd("CC=gcc CFLAGS=-O2 make");
    assert_eq!(cmd.assignments.len(), 2);
    assert_eq!(cmd.arguments.len(), 1);
    assert_eq!(word_literal(&cmd.arguments[0]), "make");
}

#[skuld::test]
fn reserved_words_as_arguments() {
    let cmd = first_cmd("echo if then else fi do done while until for case esac in");
    assert_eq!(cmd.arguments.len(), 13);
}

#[skuld::test]
fn background_and_sequential() {
    let prog = parse_ok("cmd1 & cmd2; cmd3 &");
    assert_eq!(prog.lines[0].len(), 3);
    assert_eq!(prog.lines[0][0].mode, ExecutionMode::Background);
    assert_eq!(prog.lines[0][1].mode, ExecutionMode::Terminated);
    assert_eq!(prog.lines[0][2].mode, ExecutionMode::Background);
}

#[skuld::test]
fn semicolon_separated_commands_on_one_line() {
    let prog = parse_ok("echo a; echo b; echo c");
    assert_eq!(prog.lines[0].len(), 3);
}

#[skuld::test]
fn script_with_multiple_commands() {
    let input = r#"#!/bin/sh
echo "Starting..."
cd /tmp
ls -la
echo "Done""#;
    let prog = parse_ok(input);
    let total: usize = prog.lines.iter().map(|l| l.len()).sum();
    assert!(total >= 3);
}

// Line boundary tests -------------------------------------------------------------------------------------------------

#[skuld::test]
fn line_boundary_semicolon_same_line() {
    // "a; b" → 1 line with 2 statements
    let prog = parse_ok("echo a; echo b");
    assert_eq!(prog.lines.len(), 1);
    assert_eq!(prog.lines[0].len(), 2);
}

#[skuld::test]
fn line_boundary_newline() {
    // "a\nb" → 2 lines with 1 statement each
    let prog = parse_ok("echo a\necho b");
    assert_eq!(prog.lines.len(), 2);
    assert_eq!(prog.lines[0].len(), 1);
    assert_eq!(prog.lines[1].len(), 1);
}

#[skuld::test]
fn line_boundary_semicolon_then_newline() {
    // "a;\nb" → 2 lines: line 1 has 1 Terminated stmt, line 2 has 1 Sequential stmt
    let prog = parse_ok("echo a;\necho b");
    assert_eq!(prog.lines.len(), 2);
    assert_eq!(prog.lines[0].len(), 1);
    assert_eq!(prog.lines[0][0].mode, ExecutionMode::Terminated);
    assert_eq!(prog.lines[1].len(), 1);
    assert_eq!(prog.lines[1][0].mode, ExecutionMode::Sequential);
}

#[skuld::test]
fn line_boundary_mixed() {
    // "a; b\nc; d\ne" → 3 lines
    let prog = parse_ok("echo a; echo b\necho c; echo d\necho e");
    assert_eq!(prog.lines.len(), 3);
    assert_eq!(prog.lines[0].len(), 2);
    assert_eq!(prog.lines[1].len(), 2);
    assert_eq!(prog.lines[2].len(), 1);
}

#[skuld::test]
fn empty_program() {
    assert!(parse_ok("").lines.is_empty());
}

#[skuld::test]
fn whitespace_only_program() {
    assert!(parse_ok("   \n\n  \n").lines.is_empty());
}

#[skuld::test]
fn comment_only() {
    assert!(parse_ok("# this is a comment").lines.is_empty());
}

#[skuld::test]
fn comment_after_command() {
    let prog = parse_ok("echo hello # comment\necho world");
    assert_eq!(prog.lines.len(), 2);
}

#[skuld::test]
fn posix_function_definition() {
    let e = first_expr("myfunc() { echo hello; }");
    if let Expression::FunctionDef(f) = &e {
        assert_eq!(f.name, "myfunc");
        assert!(matches!(f.body.as_ref(), CompoundCommand::BraceGroup { .. }));
    } else {
        panic!("expected FunctionDef, got {e:?}");
    }
}

#[skuld::test]
fn negated_command() {
    assert!(matches!(first_expr("! cmd"), Expression::Not(_)));
}

#[skuld::test]
fn negated_pipeline() {
    if let Expression::Not(inner) = &first_expr("! cmd1 | cmd2") {
        assert!(matches!(inner.as_ref(), Expression::Pipe { .. }));
    } else {
        panic!("expected Not");
    }
}

#[skuld::test]
fn assignment_with_quoted_value() {
    let cmd = first_cmd(r#"FOO="hello $USER""#);
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.assignments[0].name, "FOO");
}

#[skuld::test]
fn case_inside_command_substitution() {
    // ) in a case pattern inside $() must not close the command substitution.
    let prog = parse_ok("echo $(case x in a) echo yes;; esac)");
    assert_eq!(prog.lines[0].len(), 1);
}

// Append assignment (+=) =============================================================

#[skuld::test]
fn append_scalar_assignment() {
    let cmd = first_cmd("s+=foo");
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.assignments[0].name, "s");
    assert!(cmd.assignments[0].append);
    assert_eq!(cmd.arguments.len(), 0);
}

#[skuld::test]
fn append_array_assignment() {
    let prog = thaum::parse_with("a+=(x y)", thaum::Dialect::Bash).unwrap();
    let cmd = match prog.lines[0][0].expression {
        Expression::Command(ref c) => c,
        _ => panic!("expected Command"),
    };
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.assignments[0].name, "a");
    assert!(cmd.assignments[0].append);
    assert!(matches!(cmd.assignments[0].value, AssignmentValue::BashArray(_)));
}

#[skuld::test]
fn append_indexed_assignment() {
    let prog = thaum::parse_with("a[1]+=z", thaum::Dialect::Bash).unwrap();
    let cmd = match prog.lines[0][0].expression {
        Expression::Command(ref c) => c,
        _ => panic!("expected Command"),
    };
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.assignments[0].name, "a");
    assert_eq!(cmd.assignments[0].index, Some("1".to_string()));
    assert!(cmd.assignments[0].append);
}

#[skuld::test]
fn plus_equals_not_assignment_for_invalid_name() {
    // '123+=bar' has an invalid name, so it should be treated as an argument.
    let cmd = first_cmd("123+=bar");
    assert_eq!(cmd.assignments.len(), 0);
    assert_eq!(cmd.arguments.len(), 1);
}
