//! Parser integration tests covering commands, pipelines, compound statements,
//! redirections, word expansion, and Bash extensions.

use super::*;
use pretty_assertions::assert_eq;

testutil::default_labels!(lex, parse);

fn parse_ok(input: &str) -> Program {
    parse(input).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", input, e))
}

fn first_stmt(input: &str) -> Statement {
    parse_ok(input).lines.into_iter().flatten().next().unwrap()
}

fn first_expr(input: &str) -> Expression {
    first_stmt(input).expression
}

fn first_cmd(input: &str) -> Command {
    match first_expr(input) {
        Expression::Command(c) => c,
        other => panic!("expected Command, got {:?}", other),
    }
}

fn first_compound(input: &str) -> CompoundCommand {
    match first_expr(input) {
        Expression::Compound { body, .. } => body,
        other => panic!("expected Compound, got {:?}", other),
    }
}

// Simple commands -----------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_single_word_command() {
    let cmd = first_cmd("ls");
    assert_eq!(cmd.arguments.len(), 1);
    assert_eq!(
        cmd.arguments[0],
        Argument::Word(Word {
            parts: vec![Fragment::Literal("ls".into())],
            span: cmd.arguments[0].span(),
        })
    );
    assert!(cmd.assignments.is_empty());
    assert!(cmd.redirects.is_empty());
}

#[testutil::test]
fn parse_command_with_args() {
    let cmd = first_cmd("echo hello world");
    assert_eq!(cmd.arguments.len(), 3);
}

#[testutil::test]
fn parse_assignment_only() {
    let cmd = first_cmd("FOO=bar");
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.assignments[0].name, "FOO");
    assert!(cmd.arguments.is_empty());
}

#[testutil::test]
fn parse_assignment_before_command() {
    let cmd = first_cmd("FOO=bar echo hello");
    assert_eq!(cmd.assignments.len(), 1);
    assert_eq!(cmd.arguments.len(), 2);
}

#[testutil::test]
fn parse_multiple_assignments() {
    let cmd = first_cmd("A=1 B=2 cmd");
    assert_eq!(cmd.assignments.len(), 2);
    assert_eq!(cmd.arguments.len(), 1);
}

// Pipelines -----------------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_simple_pipeline() {
    assert!(matches!(first_expr("ls | grep foo"), Expression::Pipe { .. }));
}

#[testutil::test]
fn parse_multi_stage_pipeline() {
    let e = first_expr("a | b | c | d");
    if let Expression::Pipe { left, .. } = &e {
        if let Expression::Pipe { left, .. } = left.as_ref() {
            assert!(matches!(left.as_ref(), Expression::Pipe { .. }));
        } else {
            panic!("expected nested Pipe");
        }
    } else {
        panic!("expected Pipe");
    }
}

#[testutil::test]
fn parse_negated_pipeline() {
    assert!(matches!(first_expr("! cmd"), Expression::Not(_)));
}

#[testutil::test]
fn parse_negated_pipe() {
    if let Expression::Not(inner) = &first_expr("! a | b") {
        assert!(matches!(inner.as_ref(), Expression::Pipe { .. }));
    } else {
        panic!("expected Not");
    }
}

// And-Or --------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_and() {
    assert!(matches!(first_expr("a && b"), Expression::And { .. }));
}

#[testutil::test]
fn parse_or() {
    assert!(matches!(first_expr("a || b"), Expression::Or { .. }));
}

#[testutil::test]
fn parse_mixed_and_or() {
    if let Expression::Or { left, .. } = &first_expr("a && b || c") {
        assert!(matches!(left.as_ref(), Expression::And { .. }));
    } else {
        panic!("expected Or");
    }
}

#[testutil::test]
fn parse_pipe_binds_tighter_than_and() {
    if let Expression::And { left, right, .. } = &first_expr("a | b && c | d") {
        assert!(matches!(left.as_ref(), Expression::Pipe { .. }));
        assert!(matches!(right.as_ref(), Expression::Pipe { .. }));
    } else {
        panic!("expected And");
    }
}

// Execution modes -----------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_semicolon_list() {
    let prog = parse_ok("a; b");
    assert_eq!(prog.lines.len(), 1);
    assert_eq!(prog.lines[0].len(), 2);
    assert_eq!(prog.lines[0][0].mode, ExecutionMode::Terminated);
    assert_eq!(prog.lines[0][1].mode, ExecutionMode::Sequential);
}

#[testutil::test]
fn parse_background() {
    let prog = parse_ok("cmd &");
    assert_eq!(prog.lines[0].len(), 1);
    assert_eq!(prog.lines[0][0].mode, ExecutionMode::Background);
}

#[testutil::test]
fn parse_background_then_foreground() {
    let prog = parse_ok("a & b");
    assert_eq!(prog.lines[0].len(), 2);
    assert_eq!(prog.lines[0][0].mode, ExecutionMode::Background);
    assert_eq!(prog.lines[0][1].mode, ExecutionMode::Sequential);
}

#[testutil::test]
fn parse_newline_separator() {
    let prog = parse_ok("a\nb");
    assert_eq!(prog.lines.len(), 2);
}

// Redirections --------------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_input_redirect() {
    let cmd = first_cmd("cmd < file");
    assert_eq!(cmd.redirects.len(), 1);
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Input(_)));
}

#[testutil::test]
fn parse_output_redirect() {
    let cmd = first_cmd("cmd > file");
    assert_eq!(cmd.redirects.len(), 1);
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Output(_)));
}

#[testutil::test]
fn parse_fd_redirect() {
    let cmd = first_cmd("cmd 2>&1");
    assert_eq!(cmd.redirects.len(), 1);
    assert_eq!(cmd.redirects[0].fd, Some(2));
    assert!(matches!(&cmd.redirects[0].kind, RedirectKind::DupOutput(_)));
}

#[testutil::test]
fn parse_multiple_redirects() {
    let cmd = first_cmd("cmd < in > out 2>> err");
    assert_eq!(cmd.redirects.len(), 3);
}

// Compound commands ---------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_if_then_fi() {
    assert!(matches!(
        first_compound("if true; then echo yes; fi"),
        CompoundCommand::IfClause { .. }
    ));
}

#[testutil::test]
fn parse_if_then_else_fi() {
    if let CompoundCommand::IfClause { else_body, .. } = first_compound("if true; then echo yes; else echo no; fi") {
        assert!(else_body.is_some());
    } else {
        panic!("expected if clause");
    }
}

#[testutil::test]
fn parse_if_elif_else_fi() {
    if let CompoundCommand::IfClause { elifs, else_body, .. } =
        first_compound("if a; then b; elif c; then d; elif e; then f; else g; fi")
    {
        assert_eq!(elifs.len(), 2);
        assert!(else_body.is_some());
    } else {
        panic!("expected if clause");
    }
}

#[testutil::test]
fn parse_while_loop() {
    assert!(matches!(
        first_compound("while true; do echo loop; done"),
        CompoundCommand::WhileClause { .. }
    ));
}

#[testutil::test]
fn parse_until_loop() {
    assert!(matches!(
        first_compound("until false; do echo loop; done"),
        CompoundCommand::UntilClause { .. }
    ));
}

#[testutil::test]
fn parse_for_loop_with_list() {
    if let CompoundCommand::ForClause { variable, words, .. } = &first_compound("for i in a b c; do echo $i; done") {
        assert_eq!(variable, "i");
        assert_eq!(words.as_ref().unwrap().len(), 3);
    } else {
        panic!("expected for clause");
    }
}

#[testutil::test]
fn parse_brace_group() {
    assert!(matches!(
        first_compound("{ echo hello; }"),
        CompoundCommand::BraceGroup { .. }
    ));
}

#[testutil::test]
fn parse_subshell() {
    assert!(matches!(
        first_compound("(echo hello)"),
        CompoundCommand::Subshell { .. }
    ));
}

// Here-documents ------------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_heredoc() {
    let cmd = first_cmd("cat <<EOF\nhello world\nEOF\n");
    assert_eq!(cmd.redirects.len(), 1);
    if let RedirectKind::HereDoc { delimiter, body, .. } = &cmd.redirects[0].kind {
        assert_eq!(delimiter, "EOF");
        assert_eq!(body, "hello world\n");
    } else {
        panic!("expected heredoc");
    }
}

// Error cases ---------------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_error_unexpected_token() {
    assert!(parse(";;").is_err());
}

#[testutil::test]
fn parse_error_unclosed_if() {
    assert!(parse("if true; then echo yes").is_err());
}

#[testutil::test]
fn parse_error_unclosed_paren() {
    assert!(parse("(echo hello").is_err());
}

#[testutil::test]
fn parse_error_unclosed_brace() {
    assert!(parse("{ echo hello").is_err());
}

// Edge cases ----------------------------------------------------------------------------------------------------------

#[testutil::test]
fn parse_reserved_word_as_argument() {
    let cmd = first_cmd("echo if then else");
    assert_eq!(cmd.arguments.len(), 4);
}

#[testutil::test]
fn parse_empty_input() {
    assert!(parse_ok("").lines.is_empty());
}

#[testutil::test]
fn parse_only_newlines() {
    assert!(parse_ok("\n\n\n").lines.is_empty());
}

#[testutil::test]
fn parse_compound_redirect() {
    if let Expression::Compound { redirects, .. } = &first_expr("if true; then echo yes; fi > output") {
        assert_eq!(redirects.len(), 1);
    } else {
        panic!("expected compound");
    }
}

#[testutil::test]
fn parse_pipeline_with_newlines() {
    assert!(matches!(first_expr("echo hello |\ngrep h"), Expression::Pipe { .. }));
}

#[testutil::test]
fn parse_and_or_with_newlines() {
    assert!(matches!(first_expr("true &&\necho yes"), Expression::And { .. }));
}

#[testutil::test]
fn parse_for_with_newlines() {
    assert!(matches!(
        first_compound("for i in a b c\ndo\necho $i\ndone"),
        CompoundCommand::ForClause { .. }
    ));
}

#[testutil::test]
fn parse_case_with_empty_arm() {
    if let CompoundCommand::CaseClause { arms, .. } = &first_compound("case x in\na) ;;\nesac") {
        assert_eq!(arms.len(), 1);
        assert!(arms[0].body.is_empty());
    } else {
        panic!("expected case clause");
    }
}
