use super::builtins::run_builtin;
use super::Environment;

skuld::default_labels!(exec);

/// Helper: run `test` builtin with given args, return exit status.
fn test_status(args: &[&str]) -> i32 {
    let mut env = Environment::new();
    test_status_with_env(args, &mut env)
}

/// Helper: run `test` builtin with given args and a pre-configured env.
fn test_status_with_env(args: &[&str], env: &mut Environment) -> i32 {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    run_builtin("test", &owned, env, &mut sin, &mut out, &mut err).unwrap()
}

/// Helper: run `[` builtin with given args (caller must include `]`).
fn bracket_status(args: &[&str]) -> i32 {
    let mut env = Environment::new();
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut sin = std::io::empty();
    let mut out = Vec::new();
    let mut err = Vec::new();
    run_builtin("[", &owned, &mut env, &mut sin, &mut out, &mut err).unwrap()
}

// 0-arg and 1-arg basics =====================================================

#[skuld::test]
fn zero_args_is_false() {
    assert_eq!(test_status(&[]), 1);
}

#[skuld::test]
fn one_arg_nonempty_is_true() {
    assert_eq!(test_status(&["hello"]), 0);
}

#[skuld::test]
fn one_arg_empty_is_false() {
    assert_eq!(test_status(&[""]), 1);
}

#[skuld::test]
fn one_arg_operator_like_strings_are_true() {
    // POSIX: single arg is a non-empty string test, even if it looks like an operator
    assert_eq!(test_status(&["="]), 0);
    assert_eq!(test_status(&["!"]), 0);
    assert_eq!(test_status(&["("]), 0);
    assert_eq!(test_status(&["]"]), 0);
}

// Unary operators (2-arg) =====================================================

#[skuld::test]
fn unary_n_nonempty() {
    assert_eq!(test_status(&["-n", "foo"]), 0);
}

#[skuld::test]
fn unary_n_empty() {
    assert_eq!(test_status(&["-n", ""]), 1);
}

#[skuld::test]
fn unary_z_empty() {
    assert_eq!(test_status(&["-z", ""]), 0);
}

#[skuld::test]
fn unary_z_nonempty() {
    assert_eq!(test_status(&["-z", "foo"]), 1);
}

#[skuld::test]
fn unary_e_exists() {
    assert_eq!(test_status(&["-e", "/"]), 0);
}

#[skuld::test]
fn unary_e_nonexistent() {
    assert_eq!(test_status(&["-e", "/nonexistent_test_path_xyz"]), 1);
}

#[skuld::test]
fn unary_a_is_alias_for_e() {
    assert_eq!(test_status(&["-a", "/"]), 0);
    assert_eq!(test_status(&["-a", "/nonexistent_test_path_xyz"]), 1);
}

#[skuld::test]
fn unary_f_regular_file() {
    // /etc/hosts is a regular file on most Unix systems
    #[cfg(unix)]
    assert_eq!(test_status(&["-f", "/etc/hosts"]), 0);
    assert_eq!(test_status(&["-f", "/"]), 1); // directory, not regular file
}

#[skuld::test]
fn unary_d_directory() {
    assert_eq!(test_status(&["-d", "/"]), 0);
    assert_eq!(test_status(&["-d", "/nonexistent_test_path_xyz"]), 1);
}

#[skuld::test]
fn unary_s_nonzero_size() {
    #[cfg(unix)]
    assert_eq!(test_status(&["-s", "/etc/hosts"]), 0);
    assert_eq!(test_status(&["-s", "/nonexistent_test_path_xyz"]), 1);
}

#[skuld::test]
fn unary_l_symlink() {
    assert_eq!(test_status(&["-L", "/nonexistent_test_path_xyz"]), 1);
    assert_eq!(test_status(&["-h", "/nonexistent_test_path_xyz"]), 1);
}

#[skuld::test]
fn unary_v_variable_is_set() {
    let mut env = Environment::new();
    let _ = env.set_var("MY_VAR", "value");
    assert_eq!(test_status_with_env(&["-v", "MY_VAR"], &mut env), 0);
    assert_eq!(test_status_with_env(&["-v", "UNSET_VAR"], &mut env), 1);
}

#[skuld::test]
fn unary_file_type_nonexistent() {
    // All file-type checks return false for nonexistent paths
    for op in &["-b", "-c", "-p", "-S", "-t", "-u", "-g", "-k", "-O", "-G", "-N"] {
        assert_eq!(test_status(&[op, "/nonexistent_test_path_xyz"]), 1, "op {op}");
    }
}

#[skuld::test]
fn negation_two_args() {
    assert_eq!(test_status(&["!", "foo"]), 1); // ! non-empty = false
    assert_eq!(test_status(&["!", ""]), 0); // ! empty = true
}

// Binary operators (3-arg) ====================================================

#[skuld::test]
fn binary_string_equals() {
    assert_eq!(test_status(&["abc", "=", "abc"]), 0);
    assert_eq!(test_status(&["abc", "=", "def"]), 1);
    assert_eq!(test_status(&["abc", "==", "abc"]), 0);
}

#[skuld::test]
fn binary_string_not_equals() {
    assert_eq!(test_status(&["abc", "!=", "def"]), 0);
    assert_eq!(test_status(&["abc", "!=", "abc"]), 1);
}

#[skuld::test]
fn binary_string_equals_is_literal_not_glob() {
    // Unlike [[ ]], test/[ uses literal comparison, not glob
    assert_eq!(test_status(&["abc", "=", "a*"]), 1);
    assert_eq!(test_status(&["a*", "=", "a*"]), 0);
}

#[skuld::test]
fn binary_int_comparisons() {
    assert_eq!(test_status(&["5", "-eq", "5"]), 0);
    assert_eq!(test_status(&["5", "-ne", "3"]), 0);
    assert_eq!(test_status(&["3", "-lt", "5"]), 0);
    assert_eq!(test_status(&["3", "-le", "3"]), 0);
    assert_eq!(test_status(&["5", "-gt", "3"]), 0);
    assert_eq!(test_status(&["5", "-ge", "5"]), 0);
    assert_eq!(test_status(&["5", "-eq", "3"]), 1);
}

// 4-arg forms =================================================================

#[skuld::test]
fn four_arg_negation() {
    assert_eq!(test_status(&["!", "a", "=", "a"]), 1); // !(a == a) = false
    assert_eq!(test_status(&["!", "a", "=", "b"]), 0); // !(a == b) = true
}

#[skuld::test]
fn four_arg_parenthesized_unary() {
    assert_eq!(test_status(&["(", "-z", "foo", ")"]), 1);
    assert_eq!(test_status(&["(", "-z", "", ")"]), 0);
}

// Logical connectives =========================================================

#[skuld::test]
fn logical_and() {
    assert_eq!(test_status(&["foo", "-a", "bar"]), 0); // both non-empty
    assert_eq!(test_status(&["foo", "-a", ""]), 1); // second empty
    assert_eq!(test_status(&["", "-a", "bar"]), 1); // first empty
}

#[skuld::test]
fn logical_or() {
    assert_eq!(test_status(&["foo", "-o", "bar"]), 0);
    assert_eq!(test_status(&["", "-o", "bar"]), 0);
    assert_eq!(test_status(&["foo", "-o", ""]), 0);
    assert_eq!(test_status(&["", "-o", ""]), 1);
}

#[skuld::test]
fn negation_of_unary() {
    // 3 args: `! -z foo` = !(is_empty("foo")) = !false = true
    assert_eq!(test_status(&["!", "-z", "foo"]), 0);
    assert_eq!(test_status(&["!", "-z", ""]), 1);
}

// Parentheses grouping ========================================================

#[skuld::test]
fn parenthesized_string() {
    assert_eq!(test_status(&["(", "foo", ")"]), 0);
    assert_eq!(test_status(&["(", "", ")"]), 1);
}

#[skuld::test]
fn parenthesized_negation() {
    assert_eq!(test_status(&["!", "(", "foo", ")"]), 1);
    assert_eq!(test_status(&["!", "(", "", ")"]), 0);
}

// Complex multi-arg expressions ===============================================

#[skuld::test]
fn complex_and_with_unary_ops() {
    // -n foo -a -n bar -> non_empty(foo) AND non_empty(bar) = true
    assert_eq!(test_status(&["-n", "foo", "-a", "-n", "bar"]), 0);
    assert_eq!(test_status(&["-n", "foo", "-a", "-z", "bar"]), 1);
}

#[skuld::test]
fn complex_or_with_unary_ops() {
    assert_eq!(test_status(&["-z", "", "-o", "-n", "bar"]), 0);
    assert_eq!(test_status(&["-z", "foo", "-o", "-z", "bar"]), 1);
}

#[skuld::test]
fn or_lower_precedence_than_and() {
    // "" -o "a" -a "b" -> "" OR ("a" AND "b") -> false OR true -> true
    assert_eq!(test_status(&["", "-o", "a", "-a", "b"]), 0);
    // "a" -a "" -o "b" -> ("a" AND "") OR "b" -> false OR true -> true
    assert_eq!(test_status(&["a", "-a", "", "-o", "b"]), 0);
}

#[skuld::test]
fn a_ambiguity_unary_vs_logical() {
    // 2 args: `-a /` -> file exists(/) = true
    assert_eq!(test_status(&["-a", "/"]), 0);
    // 5 args: `-a / -a -a /` -> exists(/) AND exists(/) = true
    assert_eq!(test_status(&["-a", "/", "-a", "-a", "/"]), 0);
}

#[skuld::test]
fn parenthesized_and_or() {
    // ( -n foo ) -a ( -n bar )
    assert_eq!(test_status(&["(", "-n", "foo", ")", "-a", "(", "-n", "bar", ")"]), 0);
}

// Bracket syntax ==============================================================

#[skuld::test]
fn bracket_basic() {
    assert_eq!(bracket_status(&["hello", "]"]), 0);
    assert_eq!(bracket_status(&["", "]"]), 1);
}

#[skuld::test]
fn bracket_missing_close() {
    assert_eq!(bracket_status(&["hello"]), 2);
}

// Error handling (exit code 2) ================================================

#[skuld::test]
fn error_unknown_unary_op() {
    // `test -Q foo` — 2 args, `-Q` is not a valid unary operator. POSIX says
    // unspecified. Bash treats as syntax error (exit 2).
    assert_eq!(test_status(&["-Q", "foo"]), 2);
}

#[skuld::test]
fn error_unclosed_paren() {
    assert_eq!(test_status(&["(", "foo"]), 2);
}

#[skuld::test]
fn error_extra_args() {
    // `test foo bar baz quux blah` with no logical operators -> syntax error
    assert_eq!(test_status(&["foo", "bar", "baz", "quux", "blah"]), 2);
}
