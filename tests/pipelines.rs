mod common;

use common::*;
use shell_parser::ast::*;

#[test]
fn pipeline_two_commands() {
    assert!(matches!(
        first_expr("ls | grep foo"),
        Expression::Pipe { .. }
    ));
}

#[test]
fn pipeline_four_commands() {
    let e = first_expr("cat file.txt | grep pattern | sort | uniq -c");
    if let Expression::Pipe { right, .. } = &e {
        if let Expression::Command(c) = right.as_ref() {
            assert_eq!(word_literal(&c.arguments[0]), "uniq");
        } else {
            panic!("expected Command at right");
        }
    } else {
        panic!("expected Pipe");
    }
}

#[test]
fn pipeline_with_redirects() {
    let e = first_expr("grep error log.txt 2>/dev/null | wc -l");
    if let Expression::Pipe { left, .. } = &e {
        if let Expression::Command(c) = left.as_ref() {
            assert_eq!(c.redirects.len(), 1);
            assert_eq!(c.redirects[0].fd, Some(2));
        } else {
            panic!("expected Command on left");
        }
    } else {
        panic!("expected Pipe");
    }
}

#[test]
fn and_or_chain() {
    let e = first_expr("mkdir -p dir && cd dir && echo done");
    if let Expression::And { left, .. } = &e {
        assert!(matches!(left.as_ref(), Expression::And { .. }));
    } else {
        panic!("expected And");
    }
}

#[test]
fn or_fallback() {
    let e = first_expr("cmd1 || cmd2 || cmd3");
    if let Expression::Or { left, .. } = &e {
        assert!(matches!(left.as_ref(), Expression::Or { .. }));
    } else {
        panic!("expected Or");
    }
}

#[test]
fn mixed_and_or_with_pipeline() {
    let e = first_expr("cmd1 | cmd2 && cmd3 | cmd4 || cmd5");
    if let Expression::Or { left, .. } = &e {
        if let Expression::And { left, right, .. } = left.as_ref() {
            assert!(matches!(left.as_ref(), Expression::Pipe { .. }));
            assert!(matches!(right.as_ref(), Expression::Pipe { .. }));
        } else {
            panic!("expected And inside Or");
        }
    } else {
        panic!("expected Or");
    }
}

#[test]
fn complex_pipeline() {
    assert!(matches!(
        first_expr("ps aux | grep nginx | grep -v grep | awk '{print $2}'"),
        Expression::Pipe { .. }
    ));
}

#[test]
fn line_continuation_in_pipeline() {
    assert!(matches!(
        first_expr("echo hello |\ngrep h |\nsort"),
        Expression::Pipe { .. }
    ));
}

#[test]
fn compound_command_in_pipeline() {
    let e = first_expr("for i in a b; do echo $i; done | sort");
    if let Expression::Pipe { left, .. } = &e {
        assert!(matches!(
            left.as_ref(),
            Expression::Compound {
                body: CompoundCommand::ForClause { .. },
                ..
            }
        ));
    } else {
        panic!("expected Pipe");
    }
}

#[test]
fn subshell_pipeline() {
    let e = first_expr("(cd /tmp && ls) | grep test");
    if let Expression::Pipe { left, .. } = &e {
        assert!(matches!(
            left.as_ref(),
            Expression::Compound {
                body: CompoundCommand::Subshell { .. },
                ..
            }
        ));
    } else {
        panic!("expected Pipe");
    }
}
