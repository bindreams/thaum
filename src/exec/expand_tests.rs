use super::*;
use crate::ast::{Fragment, GlobChar, ParamOp, ParameterExpansion, Word};
use crate::span::Span;

skuld::default_labels!(exec);

fn dummy_span() -> Span {
    Span::new(0, 0)
}

fn make_word(parts: Vec<Fragment>) -> Word {
    Word {
        parts,
        span: dummy_span(),
    }
}

#[skuld::test]
fn expand_literal() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::Literal("hello".into())]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "hello");
}

#[skuld::test]
fn expand_single_quoted() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::SingleQuoted("don't expand $VAR".into())]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "don't expand $VAR");
}

#[skuld::test]
fn expand_concatenated_fragments() {
    let mut env = Environment::new();
    env.set_var("NAME", "world").unwrap();
    let word = make_word(vec![
        Fragment::Literal("hello_".into()),
        Fragment::Parameter(ParameterExpansion::Simple("NAME".into())),
    ]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "hello_world");
}

#[skuld::test]
fn expand_double_quoted_with_param() {
    let mut env = Environment::new();
    env.set_var("X", "value").unwrap();
    let word = make_word(vec![Fragment::DoubleQuoted(vec![
        Fragment::Literal("pre_".into()),
        Fragment::Parameter(ParameterExpansion::Simple("X".into())),
        Fragment::Literal("_post".into()),
    ])]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "pre_value_post");
}

#[skuld::test]
fn expand_tilde_alone() {
    let mut env = Environment::new();
    env.set_var("HOME", "/home/user").unwrap();
    let word = make_word(vec![Fragment::TildePrefix(String::new())]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "/home/user");
}

#[skuld::test]
fn expand_tilde_no_home() {
    let mut env = Environment::new();
    env.unset_var("HOME").unwrap();
    let word = make_word(vec![Fragment::TildePrefix(String::new())]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "~");
}

#[skuld::test]
fn expand_unset_variable() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Simple(
        "NONEXISTENT".into(),
    ))]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "");
}

#[skuld::test]
fn expand_special_param_question_mark() {
    let mut env = Environment::new();
    env.set_last_exit_status(42);
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Simple("?".into()))]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "42");
}

#[skuld::test]
fn expand_param_default() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "UNSET".into(),
        indirect: false,
        operator: Some(ParamOp::Default),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("fallback".into())]))),
    })]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "fallback");
}

#[skuld::test]
fn expand_param_default_when_set() {
    let mut env = Environment::new();
    env.set_var("SET", "actual").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "SET".into(),
        indirect: false,
        operator: Some(ParamOp::Default),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("fallback".into())]))),
    })]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "actual");
}

#[skuld::test]
fn expand_param_error_when_unset() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "MISSING".into(),
        indirect: false,
        operator: Some(ParamOp::Error),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("var is required".into())]))),
    })]);
    let err = expand_word(&word, &mut env).unwrap_err();
    assert!(matches!(err, ExecError::BadSubstitution(_)));
}

#[skuld::test]
fn expand_param_alternative() {
    let mut env = Environment::new();
    env.set_var("SET", "anything").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "SET".into(),
        indirect: false,
        operator: Some(ParamOp::Alternative),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("alt".into())]))),
    })]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "alt");
}

#[skuld::test]
fn expand_param_alternative_when_unset() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "UNSET".into(),
        indirect: false,
        operator: Some(ParamOp::Alternative),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("alt".into())]))),
    })]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "");
}

#[skuld::test]
fn expand_param_length() {
    let mut env = Environment::new();
    env.set_var("STR", "hello").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "STR".into(),
        indirect: false,
        operator: Some(ParamOp::Length),
        argument: None,
    })]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "5");
}

#[skuld::test]
fn expand_glob_literal_outside_quotes() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::Glob(GlobChar::Star), Fragment::Literal(".txt".into())]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "*.txt");
}

#[skuld::test]
fn expand_glob_literal_inside_double_quotes() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::DoubleQuoted(vec![Fragment::Glob(GlobChar::Star)])]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "*");
}

#[skuld::test]
fn expand_param_default_assign_when_unset() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "UNSET".into(),
        indirect: false,
        operator: Some(ParamOp::DefaultAssign),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("assigned".into())]))),
    })]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "assigned");
    assert_eq!(env.get_var("UNSET"), Some("assigned"));
}

#[skuld::test]
fn expand_param_default_assign_when_set() {
    let mut env = Environment::new();
    env.set_var("SET", "existing").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "SET".into(),
        indirect: false,
        operator: Some(ParamOp::DefaultAssign),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("fallback".into())]))),
    })]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "existing");
    assert_eq!(env.get_var("SET"), Some("existing"));
}

#[skuld::test]
fn expand_param_default_assign_when_empty() {
    let mut env = Environment::new();
    env.set_var("EMPTY", "").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "EMPTY".into(),
        indirect: false,
        operator: Some(ParamOp::DefaultAssign),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("filled".into())]))),
    })]);
    assert_eq!(expand_word(&word, &mut env).unwrap(), "filled");
    assert_eq!(env.get_var("EMPTY"), Some("filled"));
}

#[skuld::test]
fn expand_param_trim_small_prefix() {
    let mut env = Environment::new();
    env.set_var("PATH", "/usr/bin:/usr/local/bin").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "PATH".into(),
        indirect: false,
        operator: Some(ParamOp::TrimSmallPrefix),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("*/".into())]))),
    })]);
    // Shortest prefix matching */: "/" matches, so result is "usr/bin:/usr/local/bin"
    assert_eq!(expand_word(&word, &mut env).unwrap(), "usr/bin:/usr/local/bin");
}

#[skuld::test]
fn expand_param_trim_large_prefix() {
    let mut env = Environment::new();
    env.set_var("PATH", "/usr/bin:/usr/local/bin").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "PATH".into(),
        indirect: false,
        operator: Some(ParamOp::TrimLargePrefix),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("*/".into())]))),
    })]);
    // Longest prefix matching */: "/usr/bin:/usr/local/" matches, result is "bin"
    assert_eq!(expand_word(&word, &mut env).unwrap(), "bin");
}

#[skuld::test]
fn expand_param_trim_small_suffix() {
    let mut env = Environment::new();
    env.set_var("FILE", "archive.tar.gz").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "FILE".into(),
        indirect: false,
        operator: Some(ParamOp::TrimSmallSuffix),
        argument: Some(Box::new(make_word(vec![Fragment::Literal(".*".into())]))),
    })]);
    // Shortest suffix matching .*: ".gz" matches, result is "archive.tar"
    assert_eq!(expand_word(&word, &mut env).unwrap(), "archive.tar");
}

#[skuld::test]
fn expand_param_trim_large_suffix() {
    let mut env = Environment::new();
    env.set_var("FILE", "archive.tar.gz").unwrap();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "FILE".into(),
        indirect: false,
        operator: Some(ParamOp::TrimLargeSuffix),
        argument: Some(Box::new(make_word(vec![Fragment::Literal(".*".into())]))),
    })]);
    // Longest suffix matching .*: ".tar.gz" matches, result is "archive"
    assert_eq!(expand_word(&word, &mut env).unwrap(), "archive");
}

#[skuld::test]
fn expand_param_trim_unset_var() {
    let mut env = Environment::new();
    let word = make_word(vec![Fragment::Parameter(ParameterExpansion::Complex {
        name: "UNSET".into(),
        indirect: false,
        operator: Some(ParamOp::TrimSmallPrefix),
        argument: Some(Box::new(make_word(vec![Fragment::Literal("*".into())]))),
    })]);
    // Unset var → empty string, trim has nothing to do
    assert_eq!(expand_word(&word, &mut env).unwrap(), "");
}
