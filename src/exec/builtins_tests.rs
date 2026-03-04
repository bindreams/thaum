use super::*;

skuld::default_labels!(exec);

#[skuld::test]
fn echo_simple() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin(
        "echo",
        &["hello".into(), "world".into()],
        &mut env,
        &mut sin,
        &mut out,
        &mut err,
    )
    .unwrap();
    assert_eq!(status, 0);
    assert_eq!(String::from_utf8(out).unwrap(), "hello world\n");
}

#[skuld::test]
fn echo_n_flag() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin(
        "echo",
        &["-n".into(), "no newline".into()],
        &mut env,
        &mut sin,
        &mut out,
        &mut err,
    )
    .unwrap();
    assert_eq!(status, 0);
    assert_eq!(String::from_utf8(out).unwrap(), "no newline");
}

#[skuld::test]
fn echo_no_args() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin("echo", &[], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(status, 0);
    assert_eq!(String::from_utf8(out).unwrap(), "\n");
}

#[skuld::test]
fn true_returns_zero() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin("true", &[], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(status, 0);
}

#[skuld::test]
fn false_returns_one() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin("false", &[], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(status, 1);
}

#[skuld::test]
fn colon_returns_zero() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin(":", &[], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(status, 0);
}

#[skuld::test]
fn exit_default() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let result = run_builtin("exit", &[], &mut env, &mut sin, &mut out, &mut err);
    assert!(matches!(result, Err(ExecError::ExitRequested(0))));
}

#[skuld::test]
fn exit_with_code() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let result = run_builtin("exit", &["42".into()], &mut env, &mut sin, &mut out, &mut err);
    assert!(matches!(result, Err(ExecError::ExitRequested(42))));
}

#[skuld::test]
fn export_sets_and_exports() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    run_builtin("export", &["FOO=bar".into()], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(env.get_var("FOO"), Some("bar"));
    assert!(env.is_exported("FOO"));
}

#[skuld::test]
fn unset_removes_var() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    env.set_var("X", "val").unwrap();
    run_builtin("unset", &["X".into()], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(env.get_var("X"), None);
}

#[skuld::test]
fn shift_removes_params() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    env.set_positional_params(vec!["a".into(), "b".into(), "c".into()]);
    run_builtin("shift", &[], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(env.positional_params(), &["b".to_string(), "c".to_string()]);
}

#[skuld::test]
fn test_string_non_empty() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin("test", &["hello".into()], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(status, 0);
}

#[skuld::test]
fn test_string_empty() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin("test", &["".into()], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(status, 1);
}

#[skuld::test]
fn test_string_equals() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin(
        "test",
        &["a".into(), "=".into(), "a".into()],
        &mut env,
        &mut sin,
        &mut out,
        &mut err,
    )
    .unwrap();
    assert_eq!(status, 0);
}

#[skuld::test]
fn test_int_eq() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin(
        "test",
        &["5".into(), "-eq".into(), "5".into()],
        &mut env,
        &mut sin,
        &mut out,
        &mut err,
    )
    .unwrap();
    assert_eq!(status, 0);
}

#[skuld::test]
fn test_bracket_syntax() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin(
        "[",
        &["hello".into(), "]".into()],
        &mut env,
        &mut sin,
        &mut out,
        &mut err,
    )
    .unwrap();
    assert_eq!(status, 0);
}

#[skuld::test]
fn test_bracket_missing_close() {
    let mut env = Environment::new();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = run_builtin("[", &["hello".into()], &mut env, &mut sin, &mut out, &mut err).unwrap();
    assert_eq!(status, 2);
}
