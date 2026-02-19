use super::*;
use crate::ast::Expression;

#[test]
fn flatten_single() {
    let expr = Expression::Command(crate::ast::Command {
        assignments: vec![],
        arguments: vec![],
        redirects: vec![],
        span: crate::span::Span::new(0, 0),
    });
    let stages = flatten_pipeline(&expr);
    assert_eq!(stages.len(), 1);
}

#[test]
fn flatten_two_stage() {
    let a = Expression::Command(crate::ast::Command {
        assignments: vec![],
        arguments: vec![],
        redirects: vec![],
        span: crate::span::Span::new(0, 0),
    });
    let b = Expression::Command(crate::ast::Command {
        assignments: vec![],
        arguments: vec![],
        redirects: vec![],
        span: crate::span::Span::new(0, 0),
    });
    let pipe = Expression::Pipe {
        left: Box::new(a),
        right: Box::new(b),
        stderr: false,
    };
    let stages = flatten_pipeline(&pipe);
    assert_eq!(stages.len(), 2);
}

#[test]
fn flatten_three_stage() {
    let a = Expression::Command(crate::ast::Command {
        assignments: vec![],
        arguments: vec![],
        redirects: vec![],
        span: crate::span::Span::new(0, 0),
    });
    let b = Expression::Command(crate::ast::Command {
        assignments: vec![],
        arguments: vec![],
        redirects: vec![],
        span: crate::span::Span::new(0, 0),
    });
    let c = Expression::Command(crate::ast::Command {
        assignments: vec![],
        arguments: vec![],
        redirects: vec![],
        span: crate::span::Span::new(0, 0),
    });
    // a | b | c → Pipe(Pipe(a, b), c)
    let pipe_ab = Expression::Pipe {
        left: Box::new(a),
        right: Box::new(b),
        stderr: false,
    };
    let pipe_abc = Expression::Pipe {
        left: Box::new(pipe_ab),
        right: Box::new(c),
        stderr: false,
    };
    let stages = flatten_pipeline(&pipe_abc);
    assert_eq!(stages.len(), 3);
}
