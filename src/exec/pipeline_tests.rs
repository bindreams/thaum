use super::*;
use crate::ast::Expression;
use crate::test_labels::EXEC;
use std::io::{Read, Write};

skuld::default_labels!(EXEC);

#[skuld::test]
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

#[skuld::test]
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

#[skuld::test]
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

/// Verify that `os_pipe()` creates a working pipe: data written to the write end
/// is readable from the read end, and closing the write end signals EOF.
#[skuld::test]
fn os_pipe_write_read() {
    let (mut read_end, mut write_end) = os_pipe().unwrap();
    write_end.write_all(b"hello\n").unwrap();
    drop(write_end);
    let mut buf = String::new();
    read_end.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "hello\n");
}
