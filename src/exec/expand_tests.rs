use super::*;
use crate::ast::{Fragment, GlobChar, ParamOp, ParameterExpansion, Word};
use crate::span::Span;

fn dummy_span() -> Span {
    Span::new(0, 0)
}

fn make_word(parts: Vec<Fragment>) -> Word {
    Word {
        parts,
        span: dummy_span(),
    }
}

#[test]
fn expand_literal() {
    let env = Environment::new();
    let word = make_word(vec![Fragment::Literal("hello".into())]);
    assert_eq!(expand_word(&word, &env).unwrap(), "hello");
}

#[test]
fn expand_single_quoted() {
    let env = Environment::new();
    let word = make_word(vec![Fragment::SingleQuoted("don't expand $VAR".into())]);
    assert_eq!(expand_word(&word, &env).unwrap(), "don't expand $VAR");
}

#[test]
fn expand_concatenated_fragments() {
    let mut env = Environment::new();
    env.set_var("NAME", "world").unwrap();
    let word = make_word(vec![
        Fragment::Literal("hello_".into()),
        Fragment::Parameter(ParameterExpansion::Simple("NAME".into())),
    ]);
    assert_eq!(expand_word(&word, &env).unwrap(), "hello_world");
}

#[test]
fn expand_double_quoted_with_param() {
    let mut env = Environment::new();
    env.set_var("X", "value").unwrap();
    let word = make_word(vec![Fragment::DoubleQuoted(vec![
        Fragment::Literal("pre_".into()),
        Fragment::Parameter(ParameterExpansion::Simple("X".into())),
        Fragment::Literal("_post".into()),
    ])]);
    assert_eq!(expand_word(&word, &env).unwrap(), "pre_value_post");
}

#[test]
fn expand_tilde_alone() {
    let mut env = Environment::new();
    env.set_var("HOME", "/home/user").unwrap();
    let word = make_word(vec![Fragment::TildePrefix(String::new())]);
    assert_eq!(expand_word(&word, &env).unwrap(), "/home/user");
}

#[test]
fn expand_tilde_no_home() {
    let mut env = Environment::new();
    env.unset_var("HOME").unwrap();
    let word = make_word(vec![Fragment::TildePrefix(String::new())]);
    assert_eq!(expand_word(&word, &env).unwrap(), "~");
}

#[test]
fn expand_unset_variable() {
    let env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Simple(
        "NONEXISTENT".into(),
    ))]);
    assert_eq!(expand_word(&word, &env).unwrap(), "");
}

#[test]
fn expand_special_param_question_mark() {
    let mut env = Environment::new();
    env.set_last_exit_status(42);
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Simple(
        "?".into(),
    ))]);
    assert_eq!(expand_word(&word, &env).unwrap(), "42");
}

#[test]
fn expand_param_default() {
    let env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "UNSET".into(),
        operator: Some(ParamOp::Default),
        argument: Some(Box::new(make_word(vec![Fragment::Literal(
            "fallback".into(),
        )]))),
    })]);
    assert_eq!(expand_word(&word, &env).unwrap(), "fallback");
}

#[test]
fn expand_param_default_when_set() {
    let mut env = Environment::new();
    env.set_var("SET", "actual").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "SET".into(),
        operator: Some(ParamOp::Default),
        argument: Some(Box::new(make_word(vec![Fragment::Literal(
            "fallback".into(),
        )]))),
    })]);
    assert_eq!(expand_word(&word, &env).unwrap(), "actual");
}

#[test]
fn expand_param_error_when_unset() {
    let env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "MISSING".into(),
        operator: Some(ParamOp::Error),
        argument: Some(Box::new(make_word(vec![Fragment::Literal(
            "var is required".into(),
        )]))),
    })]);
    let err = expand_word(&word, &env).unwrap_err();
    assert!(matches!(err, ExecError::BadSubstitution(_)));
}

#[test]
fn expand_param_alternative() {
    let mut env = Environment::new();
    env.set_var("SET", "anything").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "SET".into(),
        operator: Some(ParamOp::Alternative),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("alt".into())]))),
    })]);
    assert_eq!(expand_word(&word, &env).unwrap(), "alt");
}

#[test]
fn expand_param_alternative_when_unset() {
    let env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "UNSET".into(),
        operator: Some(ParamOp::Alternative),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("alt".into())]))),
    })]);
    assert_eq!(expand_word(&word, &env).unwrap(), "");
}

#[test]
fn expand_param_length() {
    let mut env = Environment::new();
    env.set_var("STR", "hello").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "STR".into(),
        operator: Some(ParamOp::Length),
        argument: None,
    })]);
    assert_eq!(expand_word(&word, &env).unwrap(), "5");
}

#[test]
fn expand_glob_literal_outside_quotes() {
    let env = Environment::new();
    let word = make_word(vec![
        Fragment::Glob(GlobChar::Star),
        Fragment::Literal(".txt".into()),
    ]);
    assert_eq!(expand_word(&word, &env).unwrap(), "*.txt");
}

#[test]
fn expand_glob_literal_inside_double_quotes() {
    let env = Environment::new();
    let word = make_word(vec![Fragment::DoubleQuoted(vec![Fragment::Glob(
        GlobChar::Star,
    )])]);
    assert_eq!(expand_word(&word, &env).unwrap(), "*");
}
