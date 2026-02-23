use super::*;

#[test]
fn get_set_var() {
    let mut env = Environment::new();
    assert_eq!(env.get_var("FOO"), None);

    env.set_var("FOO", "bar").unwrap();
    assert_eq!(env.get_var("FOO"), Some("bar"));

    env.set_var("FOO", "baz").unwrap();
    assert_eq!(env.get_var("FOO"), Some("baz"));
}

#[test]
fn unset_var() {
    let mut env = Environment::new();
    env.set_var("FOO", "bar").unwrap();
    env.unset_var("FOO").unwrap();
    assert_eq!(env.get_var("FOO"), None);
}

#[test]
fn readonly_prevents_set() {
    let mut env = Environment::new();
    env.set_var("FOO", "bar").unwrap();
    env.set_readonly("FOO");

    let err = env.set_var("FOO", "baz").unwrap_err();
    assert!(matches!(err, ExecError::ReadonlyVariable(_)));

    // Value unchanged
    assert_eq!(env.get_var("FOO"), Some("bar"));
}

#[test]
fn readonly_prevents_unset() {
    let mut env = Environment::new();
    env.set_var("FOO", "bar").unwrap();
    env.set_readonly("FOO");

    let err = env.unset_var("FOO").unwrap_err();
    assert!(matches!(err, ExecError::ReadonlyVariable(_)));
}

#[test]
fn export_var() {
    let mut env = Environment::new();
    env.set_var("FOO", "bar").unwrap();
    assert!(!env.is_exported("FOO"));

    env.export_var("FOO");
    assert!(env.is_exported("FOO"));

    let exported = env.exported_vars();
    assert!(exported.iter().any(|(k, v)| k == "FOO" && v == "bar"));
}

#[test]
fn export_nonexistent_creates_empty() {
    let mut env = Environment::new();
    env.export_var("NEW");
    assert_eq!(env.get_var("NEW"), Some(""));
    assert!(env.is_exported("NEW"));
}

#[test]
fn special_params() {
    let mut env = Environment::new();
    env.set_last_exit_status(42);
    assert_eq!(env.get_special("?"), Some("42".to_string()));

    env.set_positional_params(vec!["a".into(), "b".into(), "c".into()]);
    assert_eq!(env.get_special("#"), Some("3".to_string()));
    assert_eq!(env.get_special("1"), Some("a".to_string()));
    assert_eq!(env.get_special("2"), Some("b".to_string()));
    assert_eq!(env.get_special("3"), Some("c".to_string()));
    assert_eq!(env.get_special("4"), None);
    assert_eq!(env.get_special("@"), Some("a b c".to_string()));
    assert_eq!(env.get_special("*"), Some("a b c".to_string()));
    assert_eq!(env.get_special("0"), Some("sh".to_string()));
}

#[test]
fn scope_push_pop_does_not_restore_vars_in_posix() {
    let mut env = Environment::new();
    env.set_var("X", "outer").unwrap();

    env.push_scope(vec!["arg1".into()]);
    assert_eq!(env.get_special("1"), Some("arg1".to_string()));

    env.set_var("X", "inner").unwrap();
    assert_eq!(env.get_var("X"), Some("inner"));

    env.set_var("Y", "new").unwrap();
    assert_eq!(env.get_var("Y"), Some("new"));

    env.pop_scope();

    // In POSIX mode, variables set in functions persist after return.
    assert_eq!(env.get_var("X"), Some("inner"));
    assert_eq!(env.get_var("Y"), Some("new"));
    // Positional params are restored
    assert_eq!(env.get_special("1"), None);
}

#[test]
fn scope_push_pop_restores_positional() {
    let mut env = Environment::new();
    env.set_positional_params(vec!["orig1".into(), "orig2".into()]);

    env.push_scope(vec!["func_arg".into()]);
    assert_eq!(env.positional_params(), &["func_arg".to_string()]);

    env.pop_scope();
    assert_eq!(env.positional_params(), &["orig1".to_string(), "orig2".to_string()]);
}

#[test]
fn default_ifs() {
    let env = Environment::new();
    assert_eq!(env.ifs(), " \t\n");
}

#[test]
fn cwd_is_set() {
    let env = Environment::new();
    assert!(env.cwd().is_absolute());
}

#[test]
fn pid_is_set() {
    let env = Environment::new();
    assert_eq!(env.get_special("$"), Some(std::process::id().to_string()));
}
