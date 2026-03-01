use super::*;

testutil::default_labels!(exec);

#[testutil::test]
fn get_set_var() {
    let mut env = Environment::new();
    assert_eq!(env.get_var("FOO"), None);

    env.set_var("FOO", "bar").unwrap();
    assert_eq!(env.get_var("FOO"), Some("bar"));

    env.set_var("FOO", "baz").unwrap();
    assert_eq!(env.get_var("FOO"), Some("baz"));
}

#[testutil::test]
fn unset_var() {
    let mut env = Environment::new();
    env.set_var("FOO", "bar").unwrap();
    env.unset_var("FOO").unwrap();
    assert_eq!(env.get_var("FOO"), None);
}

#[testutil::test]
fn readonly_prevents_set() {
    let mut env = Environment::new();
    env.set_var("FOO", "bar").unwrap();
    env.set_readonly("FOO");

    let err = env.set_var("FOO", "baz").unwrap_err();
    assert!(matches!(err, ExecError::ReadonlyVariable(_)));

    // Value unchanged
    assert_eq!(env.get_var("FOO"), Some("bar"));
}

#[testutil::test]
fn readonly_prevents_unset() {
    let mut env = Environment::new();
    env.set_var("FOO", "bar").unwrap();
    env.set_readonly("FOO");

    let err = env.unset_var("FOO").unwrap_err();
    assert!(matches!(err, ExecError::ReadonlyVariable(_)));
}

#[testutil::test]
fn export_var() {
    let mut env = Environment::new();
    env.set_var("FOO", "bar").unwrap();
    assert!(!env.is_exported("FOO"));

    env.export_var("FOO");
    assert!(env.is_exported("FOO"));

    let exported = env.exported_vars();
    assert!(exported.iter().any(|(k, v)| k == "FOO" && v == "bar"));
}

#[testutil::test]
fn export_nonexistent_creates_empty() {
    let mut env = Environment::new();
    env.export_var("NEW");
    assert_eq!(env.get_var("NEW"), Some(""));
    assert!(env.is_exported("NEW"));
}

#[testutil::test]
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

#[testutil::test]
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

#[testutil::test]
fn scope_push_pop_restores_positional() {
    let mut env = Environment::new();
    env.set_positional_params(vec!["orig1".into(), "orig2".into()]);

    env.push_scope(vec!["func_arg".into()]);
    assert_eq!(env.positional_params(), &["func_arg".to_string()]);

    env.pop_scope();
    assert_eq!(env.positional_params(), &["orig1".to_string(), "orig2".to_string()]);
}

#[testutil::test]
fn default_ifs() {
    let env = Environment::new();
    assert_eq!(env.ifs(), " \t\n");
}

#[testutil::test]
fn cwd_is_set() {
    let env = Environment::new();
    assert!(env.cwd().is_absolute());
}

#[testutil::test]
fn pid_is_set() {
    let env = Environment::new();
    assert_eq!(env.get_special("$"), Some(std::process::id().to_string()));
}

#[testutil::test]
fn special_param_dash_default() {
    let env = Environment::new();
    let flags = env.get_special("-").expect("$- should return Some");
    // Default env has no options enabled, so flags should be empty or contain only
    // default-on flags.
    for c in flags.chars() {
        assert!(c.is_ascii_alphabetic(), "unexpected char in $-: {:?}", c);
    }
}

#[testutil::test]
fn special_param_dash_with_errexit() {
    let mut env = Environment::new();
    env.set_errexit(true);
    let flags = env.get_special("-").unwrap();
    assert!(flags.contains('e'), "flags should contain 'e' when errexit is on");
}

#[testutil::test]
fn special_param_dash_with_nounset() {
    let mut env = Environment::new();
    env.set_nounset(true);
    let flags = env.get_special("-").unwrap();
    assert!(flags.contains('u'), "flags should contain 'u' when nounset is on");
}

#[testutil::test]
fn special_param_dash_with_xtrace() {
    let mut env = Environment::new();
    env.set_xtrace(true);
    let flags = env.get_special("-").unwrap();
    assert!(flags.contains('x'), "flags should contain 'x' when xtrace is on");
}

#[testutil::test]
fn special_param_dash_with_multiple_options() {
    let mut env = Environment::new();
    env.set_errexit(true);
    env.set_nounset(true);
    env.set_xtrace(true);
    let flags = env.get_special("-").unwrap();
    assert!(flags.contains('e'));
    assert!(flags.contains('u'));
    assert!(flags.contains('x'));
}

#[cfg(unix)]
#[testutil::test]
fn bash_vars_groups_is_populated() {
    let mut env = Environment::new();
    env.initialize_bash_vars();
    let groups = env.get_array_all("GROUPS");
    assert!(
        groups.is_some(),
        "GROUPS array should be set after initialize_bash_vars()"
    );
    let groups = groups.unwrap();
    assert!(!groups.is_empty(), "GROUPS should contain at least one group ID");
    // Every element should be a valid numeric group ID.
    for gid in &groups {
        assert!(gid.parse::<u32>().is_ok(), "GROUPS element {gid:?} is not a valid gid");
    }
}
