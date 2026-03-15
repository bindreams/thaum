use std::path::Path;

use thaum::Dialect;

use crate::*;

// Bash alias expansion ------------------------------------------------------------------------------------------------

#[skuld::test]
fn alias_basic() {
    let r = exec!(
        "shopt -s expand_aliases\nalias hi='echo hello'\nhi",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn alias_requires_shopt() {
    // Without shopt -s expand_aliases, aliases are defined but not expanded
    let r = exec!("alias hi='echo hello'\nhi", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0);
    assert!(
        r.stderr().contains("hi: command not found"),
        "expected 'command not found' in stderr: {}",
        r.stderr()
    );
}

#[skuld::test]
fn alias_same_line_not_expanded() {
    // alias e=echo; e one — same line, e is NOT expanded (parsed before defined)
    let r = exec!("shopt -s expand_aliases\nalias e=echo; e one", dialect = Dialect::Bash);
    assert_ne!(r.status(), 0);
    assert!(
        r.stderr().contains("e: command not found"),
        "expected 'command not found' in stderr: {}",
        r.stderr()
    );
}

#[skuld::test]
fn alias_cross_line_expanded() {
    let r = exec!(
        "shopt -s expand_aliases\nalias e=echo\ne hello",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn alias_semicolon_then_newline() {
    // alias a="echo";  ← trailing semicolon, then newline → next line sees alias
    let r = exec!(
        "shopt -s expand_aliases\nalias a=echo;\na hello",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn alias_unalias() {
    let r = exec!(
        "shopt -s expand_aliases\nalias e=echo\nunalias e\ne hello",
        dialect = Dialect::Bash
    );
    assert_ne!(r.status(), 0);
    assert!(
        r.stderr().contains("e: command not found"),
        "expected 'command not found' in stderr: {}",
        r.stderr()
    );
}

#[skuld::test]
fn alias_unalias_same_line() {
    // alias + unalias on one line; next line sees no alias
    let r = exec!(
        "shopt -s expand_aliases\nalias a=echo; unalias a\na hello",
        dialect = Dialect::Bash
    );
    assert_ne!(r.status(), 0);
    assert!(
        r.stderr().contains("a: command not found"),
        "expected 'command not found' in stderr: {}",
        r.stderr()
    );
}

#[skuld::test]
fn alias_recursive() {
    let r = exec!(
        "shopt -s expand_aliases\nalias hi='e_ hello'\nalias e_='echo __'\nhi",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "__ hello\n");
}

#[skuld::test]
fn alias_trailing_space() {
    // Alias ending with space → next word also alias-expanded
    let r = exec!(
        "shopt -s expand_aliases\nalias hi='echo '\nalias w='hello'\nhi w",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn alias_quoted_not_expanded() {
    // Quoted command name must NOT trigger alias expansion
    let r = exec!(
        "shopt -s expand_aliases\nalias hi='echo hello'\n'hi'",
        dialect = Dialect::Bash
    );
    assert_ne!(r.status(), 0);
    assert!(
        r.stderr().contains("hi: command not found"),
        "expected 'command not found' in stderr: {}",
        r.stderr()
    );
}

#[skuld::test]
fn alias_list() {
    let r = exec!("shopt -s expand_aliases\nalias e=echo\nalias", dialect = Dialect::Bash);
    let out = r.stdout();
    assert!(out.contains("alias e='echo'") || out.contains("alias e=echo"));
}

#[skuld::test]
fn alias_redefine_then_unalias() {
    // Line 2: alias a="touch"  → defines a=touch
    // Line 3: alias a="echo"; unalias a  → redefines then removes
    // Line 4: a hello  → not found (unalias took effect)
    let r = exec!(
        "shopt -s expand_aliases\nalias a=touch\nalias a=echo; unalias a\na hello",
        dialect = Dialect::Bash
    );
    assert_ne!(r.status(), 0);
    assert!(
        r.stderr().contains("a: command not found"),
        "expected 'command not found' in stderr: {}",
        r.stderr()
    );
}

#[skuld::test]
fn alias_snapshot_uses_previous_line() {
    // Line 2: alias a="echo"  → defines a=echo
    // Line 3: alias a="touch"; a hello; unalias a
    //   → snapshot for line 3 has a=echo (from before line 3 executed)
    //   → so "a hello" expands to "echo hello" (not "touch hello")
    //   → then alias a is redefined to touch, then unaliased — both during execution
    // Line 4: a hello  → not found (unalias from line 3 took effect)
    let r = exec!(
        "shopt -s expand_aliases\nalias a=echo\nalias a=touch; a hello; unalias a",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn alias_snapshot_creates_file(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    // Line 2: alias a="touch"
    // Line 3: alias a="echo"; a hello; unalias a
    //   → snapshot for line 3 has a=touch (from line 2)
    //   → "a hello" expands to "touch hello" (creates file)
    let file = dir.join("hello");
    let d = dir.to_string_lossy().replace('\\', "/");
    let tools_dir = tools.to_string_lossy();

    let script = format!("shopt -s expand_aliases\nalias a=touch\ncd {d}; alias a=echo; a hello; unalias a");
    exec!(&script, dialect = Dialect::Bash, env = &[("PATH", &*tools_dir)]);
    assert!(file.exists(), "touch hello should have created the file");
}

// Alias funkiness levels (see CONTRIBUTING.md) ========================================================================
//
// Levels 1–4 must work. Level 5 (partial compound syntax) is unsupported by design.

#[skuld::test]
fn alias_funkiness_level2_multiple_words() {
    // Level 2: alias value contains command + flags (multiple words)
    let r = exec!(
        "shopt -s expand_aliases\nalias greet='echo -n hello'\ngreet",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "hello");
}

#[skuld::test]
fn alias_funkiness_level3a_redirect_in_value(#[fixture(temp_dir)] dir: &Path) {
    // Level 3a: alias value contains a redirect — verify the file is created.
    let file = dir.join("out.txt");

    let f = file.to_string_lossy().replace('\\', "/");
    let script = format!("shopt -s expand_aliases\nalias w='echo hello >'\nw {f}");
    exec!(&script, dialect = Dialect::Bash);
    let contents = std::fs::read_to_string(&file).expect("redirect should have created file");
    assert_eq!(contents.trim(), "hello");
}

#[skuld::test]
fn alias_funkiness_level3b_command_sub_in_value() {
    // Level 3b: alias value contains $() command substitution
    let r = exec!(
        "shopt -s expand_aliases\nalias greet='echo $(echo hi)'\ngreet",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "hi\n");
}

#[skuld::test]
fn alias_funkiness_level4a_pipe_in_value(#[fixture(test_tools)] tools: &Path) {
    // Level 4: alias value contains a pipe (creates a pipeline).
    // After expansion: `echo hello | cat; echo done`
    let tools_dir = tools.to_string_lossy();
    let r = exec!(
        "shopt -s expand_aliases\nalias both='echo hello | cat; echo'\nboth done",
        dialect = Dialect::Bash,
        env = &[("PATH", &*tools_dir)]
    );
    assert_eq!(r.stdout(), "hello\ndone\n");
}

#[skuld::test]
fn alias_funkiness_level4b_semicolons_in_value() {
    // Level 4: alias value contains ; (splits into multiple commands)
    let r = exec!(
        "shopt -s expand_aliases\nalias both='echo one; echo'\nboth two",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "one\ntwo\n");
}

#[skuld::test]
fn alias_funkiness_level4c_and_chain_in_value() {
    // Level 4: alias value contains && (and-chain)
    let r = exec!(
        "shopt -s expand_aliases\nalias chk='true && echo'\nchk ok",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "ok\n");
}

// Subshell execution --------------------------------------------------------------------------------------------------

#[skuld::test]
fn subshell_basic() {
    let r = exec!("(echo hello)");
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn subshell_exit_status() {
    let r = exec!("(exit 42); echo $?");
    assert_eq!(r.stdout(), "42\n");
}

#[skuld::test]
fn subshell_variable_isolation() {
    let r = exec!("x=1; (x=2); echo $x");
    assert_eq!(r.stdout(), "1\n");
}

#[skuld::test]
fn subshell_inherits_vars() {
    let r = exec!("x=hello; (echo $x)");
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn subshell_inherits_functions() {
    let r = exec!("f() { echo hi; }; (f)");
    assert_eq!(r.stdout(), "hi\n");
}

#[skuld::test]
fn subshell_nested() {
    let r = exec!("((echo inner))");
    assert_eq!(r.stdout(), "inner\n");
}

#[skuld::test]
fn subshell_with_redirect(#[fixture(temp_dir)] dir: &Path) {
    // Redirect inside the subshell (not on the compound command).
    let file = dir.join("out.txt");

    let script = format!("(echo hello > {})", file.to_string_lossy().replace('\\', "/"));
    let r = exec!(&script);
    assert_eq!(r.stdout(), ""); // stdout went to file inside subshell
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

// Bash [[ ]] conditional ----------------------------------------------------------------------------------------------

#[skuld::test]
fn bash_cond_string_equals() {
    assert_eq!(exec!("[[ hello == hello ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_string_not_equals() {
    assert_eq!(exec!("[[ a != b ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_false() {
    assert_eq!(exec!("[[ a == b ]]", dialect = Dialect::Bash).status(), 1);
}

#[skuld::test]
fn bash_cond_string_empty() {
    assert_eq!(exec!("[[ -z '' ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_string_nonempty() {
    assert_eq!(exec!("[[ -n hello ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_file_exists() {
    assert_eq!(exec!("[[ -e /tmp ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_file_is_dir() {
    assert_eq!(exec!("[[ -d /tmp ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_file_not_exists() {
    assert_eq!(
        exec!("[[ -e /nonexistent_path_xyz ]]", dialect = Dialect::Bash).status(),
        1
    );
}

#[skuld::test]
fn bash_cond_int_eq() {
    assert_eq!(exec!("[[ 42 -eq 42 ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_int_lt() {
    assert_eq!(exec!("[[ 1 -lt 2 ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_and() {
    assert_eq!(exec!("[[ -n a && -n b ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_or() {
    assert_eq!(exec!("[[ -z '' || -n b ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_not() {
    assert_eq!(exec!("[[ ! -z hello ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_variable() {
    assert_eq!(exec!("x=hi; [[ -n $x ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_regex() {
    assert_eq!(exec!("[[ abc123 =~ [0-9]+ ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_regex_no_match() {
    assert_eq!(exec!("[[ abcdef =~ [0-9]+ ]]", dialect = Dialect::Bash).status(), 1);
}

#[skuld::test]
fn bash_cond_lexical_lt() {
    assert_eq!(exec!("[[ apple < banana ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_var_set() {
    assert_eq!(exec!("x=1; [[ -v x ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_var_unset() {
    assert_eq!(exec!("[[ -v nonexistent_var ]]", dialect = Dialect::Bash).status(), 1);
}

#[skuld::test]
fn bash_cond_in_if() {
    let r = exec!("if [[ 1 -eq 1 ]]; then echo yes; fi", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "yes\n");
}

#[skuld::test]
fn bash_cond_bare_word() {
    // Bare non-empty word is true (implicit -n)
    assert_eq!(exec!("[[ hello ]]", dialect = Dialect::Bash).status(), 0);
}

#[skuld::test]
fn bash_cond_bare_empty() {
    // Empty string is false
    assert_eq!(exec!("[[ '' ]]", dialect = Dialect::Bash).status(), 1);
}

// set -x (xtrace) -----------------------------------------------------------------------------------------------------

#[skuld::test]
fn set_x_basic() {
    // xtrace goes to stderr; stdout should only contain the echo output
    let r = exec!("set -x; echo hello");
    assert_eq!(r.stdout(), "hello\n");
    let _ = r.stderr(); // xtrace output goes to stderr; don't assert contents
    let _ = r.status();
}

#[skuld::test]
fn set_x_off() {
    let r = exec!("set -x; set +x; echo hello");
    assert_eq!(r.stdout(), "hello\n");
    let _ = r.stderr(); // xtrace output goes to stderr; don't assert contents
}

// set -u (nounset) ----------------------------------------------------------------------------------------------------

#[skuld::test]
fn set_u_unset_var() {
    let r = exec!("set -u; echo $nonexistent_xyz");
    assert_ne!(r.status(), 0);
    assert!(r.stderr().contains("unbound variable"));
}

#[skuld::test]
fn set_u_set_var() {
    let r = exec!("set -u; x=hi; echo $x");
    assert_eq!(r.stdout(), "hi\n");
}

#[skuld::test]
fn set_u_default() {
    let r = exec!("set -u; echo ${nonexistent_xyz:-fallback}");
    assert_eq!(r.stdout(), "fallback\n");
}

#[skuld::test]
fn set_u_special() {
    let r = exec!("set -u; echo $?");
    assert_eq!(r.stdout(), "0\n");
}

#[skuld::test]
fn set_u_off() {
    let r = exec!("set -u; set +u; echo ${nonexistent_xyz}done");
    assert_eq!(r.stdout(), "done\n");
}

// set -e (errexit) ----------------------------------------------------------------------------------------------------

#[skuld::test]
fn set_e_basic() {
    // false triggers errexit — "nope" is never printed
    let r = exec!("set -e; false; echo nope");
    assert_eq!(r.stdout(), "");
    assert_ne!(r.status(), 0);
}

#[skuld::test]
fn set_e_if_condition() {
    // false in if condition does NOT trigger errexit
    let r = exec!("set -e; if false; then echo then; fi; echo ok");
    assert_eq!(r.stdout(), "ok\n");
}

#[skuld::test]
fn set_e_and_chain() {
    // false on left side of && does NOT trigger errexit
    let r = exec!("set -e; false && true; echo ok");
    assert_eq!(r.stdout(), "ok\n");
}

#[skuld::test]
fn set_e_or_chain() {
    // false on left side of || does NOT trigger errexit
    let r = exec!("set -e; false || true; echo ok");
    assert_eq!(r.stdout(), "ok\n");
}

#[skuld::test]
fn set_e_not() {
    // ! false (negation) does NOT trigger errexit
    let r = exec!("set -e; ! false; echo ok");
    assert_eq!(r.stdout(), "ok\n");
}

#[skuld::test]
fn set_e_off() {
    // set +e disables errexit
    let r = exec!("set -e; set +e; false; echo ok");
    assert_eq!(r.stdout(), "ok\n");
}

// Case modification operators (${var^}, ${var^^}, ${var,}, ${var,,}) --------------------------------------------------

#[skuld::test]
fn case_mod_upper_first() {
    let r = exec!("x=hello; echo ${x^}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "Hello\n");
}

#[skuld::test]
fn case_mod_upper_all() {
    let r = exec!("x=hello; echo ${x^^}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "HELLO\n");
}

#[skuld::test]
fn case_mod_lower_first() {
    let r = exec!("x=HELLO; echo ${x,}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hELLO\n");
}

#[skuld::test]
fn case_mod_lower_all() {
    let r = exec!("x=HELLO; echo ${x,,}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn case_mod_unicode() {
    let r = exec!("x=café; echo ${x^^}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "CAFÉ\n");
}

#[skuld::test]
fn case_mod_empty() {
    let r = exec!("x=''; echo \"${x^^}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "\n");
}

#[skuld::test]
fn case_mod_unset() {
    let r = exec!("echo \"${unset_var^^}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "\n");
}

// POSIX character classes in case =====================================================================================

#[skuld::test]
fn case_char_class_upper() {
    let r = exec!(
        "case A in [[:upper:]]) echo y;; *) echo n;; esac",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "y\n");
}

#[skuld::test]
fn case_char_class_lower() {
    let r = exec!(
        "case a in [[:lower:]]) echo y;; *) echo n;; esac",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "y\n");
}

#[skuld::test]
fn case_char_class_digit() {
    let r = exec!(
        "case 5 in [[:digit:]]) echo y;; *) echo n;; esac",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "y\n");
}

#[skuld::test]
fn case_char_class_space() {
    let r = exec!("case ' ' in [[:space:]]) echo y;; *) echo n;; esac");
    assert_eq!(r.stdout(), "y\n");
}

#[skuld::test]
fn case_char_class_alpha_negated() {
    let r = exec!(
        "case 5 in [![:alpha:]]) echo y;; *) echo n;; esac",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "y\n");
}

#[skuld::test]
fn case_char_class_mixed_bracket() {
    // Class + literal in same bracket
    let r = exec!(
        "case _ in [[:alpha:]_]) echo y;; *) echo n;; esac",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "y\n");
}

#[skuld::test]
fn case_char_class_alnum_with_star() {
    let r = exec!(
        "case hello123 in [[:alnum:]]*) echo y;; *) echo n;; esac",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "y\n");
}

// Character classes in parameter expansion ============================================================================

#[skuld::test]
fn trim_char_class_alpha_prefix() {
    let r = exec!("x=hello123; echo ${x##[[:alpha:]]*}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "\n");
}

#[skuld::test]
fn trim_char_class_digit_suffix() {
    let r = exec!("x=hello123; echo ${x%%[[:digit:]]*}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

// Regex =~ with character classes =====================================================================================

#[skuld::test]
fn regex_char_class_digit() {
    assert_eq!(
        exec!("[[ abc123 =~ [[:digit:]]+ ]]", dialect = Dialect::Bash).status(),
        0
    );
}

#[skuld::test]
fn regex_char_class_alpha() {
    assert_eq!(
        exec!("[[ hello =~ ^[[:alpha:]]+$ ]]", dialect = Dialect::Bash).status(),
        0
    );
}

#[skuld::test]
fn regex_char_class_space() {
    assert_eq!(
        exec!("[[ 'hello world' =~ [[:space:]] ]]", dialect = Dialect::Bash).status(),
        0
    );
}

#[skuld::test]
fn regex_char_class_upper() {
    assert_eq!(
        exec!("[[ Hello =~ ^[[:upper:]] ]]", dialect = Dialect::Bash).status(),
        0
    );
}

// Locale sensitivity of character classes =============================================================================

#[skuld::test]
fn case_char_class_upper_accent_c_locale() {
    // In C locale, É is NOT [[:upper:]]
    let r = exec!(
        "LC_CTYPE=C; case É in [[:upper:]]) echo y;; *) echo n;; esac",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "n\n");
}

#[skuld::test]
fn case_char_class_upper_accent_utf8_locale() {
    // In UTF-8 locale, É IS [[:upper:]]
    let r = exec!(
        "LC_CTYPE=en_US.UTF-8; case É in [[:upper:]]) echo y;; *) echo n;; esac",
        dialect = Dialect::Bash
    );
    assert_eq!(r.stdout(), "y\n");
}

// Locale translation ($"...") =========================================================================================

#[skuld::test]
fn locale_quoted_no_domain() {
    // Without TEXTDOMAIN, $"..." just expands like double quotes
    let r = exec!("echo $\"hello world\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello world\n");
}

#[skuld::test]
fn locale_quoted_with_variable_no_domain() {
    // $"..." expands variables even without translation
    let r = exec!("x=test; echo $\"hello $x\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello test\n");
}

#[skuld::test]
fn locale_quoted_basic_translation() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"hello world\"",
        fixture_dir()
    );
    let r = exec!(&script, dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hallo welt\n");
}

#[skuld::test]
fn locale_quoted_with_variable_translation() {
    let script = format!(
        "USER=Claude\nTEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"hello $USER\"",
        fixture_dir()
    );
    let r = exec!(&script, dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hallo Claude\n");
}

#[skuld::test]
fn locale_quoted_missing_msgid() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"not in catalog\"",
        fixture_dir()
    );
    let r = exec!(&script, dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "not in catalog\n");
}

#[skuld::test]
fn locale_quoted_c_locale_no_translation() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=C\necho $\"hello world\"",
        fixture_dir()
    );
    let r = exec!(&script, dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello world\n");
}

#[skuld::test]
fn locale_quoted_empty_string() {
    let r = exec!("echo $\"\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "\n");
}

#[skuld::test]
fn locale_quoted_fallback_locale() {
    // LANG=de_DE.UTF-8 with .mo only in de/ directory -- should fall back
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLANG=de_DE.UTF-8\necho $\"goodbye\"",
        fixture_dir()
    );
    let r = exec!(&script, dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "auf wiedersehen\n");
}

// Parameter transformation @Q/@a/@A -----------------------------------------------------------------------------------

#[skuld::test]
fn transform_quote_simple() {
    let r = exec!("x=hello; echo \"${x@Q}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "'hello'\n");
}

#[skuld::test]
fn transform_attrs_plain() {
    let r = exec!("x=hello; echo \"${x@a}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "\n");
}

#[skuld::test]
fn transform_attrs_integer() {
    let r = exec!("declare -i n=42; echo \"${n@a}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "i\n");
}

#[skuld::test]
fn transform_attrs_exported_readonly() {
    let r = exec!("declare -rx e=test; echo \"${e@a}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "rx\n");
}

#[skuld::test]
fn transform_attrs_array() {
    let r = exec!("declare -a a; a=(1 2); echo \"${a@a}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "a\n");
}

#[skuld::test]
fn transform_attrs_assoc() {
    let r = exec!("declare -A m=([k]=v); echo \"${m@a}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "A\n");
}

#[skuld::test]
fn transform_assign_scalar() {
    let r = exec!("x=hello; echo \"${x@A}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "x='hello'\n");
}

#[skuld::test]
fn transform_assign_integer() {
    let r = exec!("declare -i n=42; echo \"${n@A}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "declare -i n='42'\n");
}

#[skuld::test]
fn transform_lower() {
    let r = exec!("x=HELLO; echo \"${x@L}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "hello\n");
}

#[skuld::test]
fn transform_upper() {
    let r = exec!("x=hello; echo \"${x@U}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "HELLO\n");
}

#[skuld::test]
fn transform_capitalize() {
    let r = exec!("x=hello; echo \"${x@u}\"", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "Hello\n");
}

// Indirect expansion ${!var[@]} ---------------------------------------------------------------------------------------

#[skuld::test]
fn indirect_array_keys() {
    let r = exec!("a=(x y z); echo ${!a[@]}", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "0 1 2\n");
}

#[skuld::test]
fn indirect_assoc_keys() {
    // Assoc array keys are unordered, so just check we get both
    let r = exec!("declare -A m; m[k]=v; m[j]=w; echo ${!m[@]}", dialect = Dialect::Bash);
    let out = r.stdout();
    let keys: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"k"));
    assert!(keys.contains(&"j"));
}

// Versioned dialect tests ---------------------------------------------------------------------------------------------

#[skuld::test]
fn bash_is_bash51() {
    // Dialect::Bash and Dialect::Bash51 produce identical options
    assert_eq!(Dialect::Bash.options(), Dialect::Bash51.options());
}

#[skuld::test]
fn bash44_has_array_empty_element_bug() {
    // In bash 4.4, ${a[@]:+foo} on array with empty element returns "foo" (bug)
    let r = exec!("a=(''); echo \"${a[@]:+foo}\"", dialect = Dialect::Bash44);
    assert_eq!(r.stdout(), "foo\n");
}

#[skuld::test]
fn bash50_fixes_array_empty_element_bug() {
    // In bash 5.0+, ${a[@]:+foo} on array with empty element returns "" (fixed)
    let r = exec!("a=(''); echo \"${a[@]:+foo}\"", dialect = Dialect::Bash50);
    assert_eq!(r.stdout(), "\n");
}

#[skuld::test]
fn bash44_rejects_transform_lower() {
    // @L is bash 5.1+ — in bash 4.4, the parser does not recognize @L as a
    // transform, so `x@L` is treated as a variable name containing `@` which
    // expands to empty (no bad-substitution error at parse time, but the
    // transform is not applied).
    let r = exec!("x=HELLO; echo \"${x@L}\"", dialect = Dialect::Bash44);
    // Without the transform, `x@L` is an undefined variable → empty
    assert_eq!(r.stdout(), "\n");
}

#[skuld::test]
fn bash50_rejects_transform_lower() {
    // @L is bash 5.1+ — same behavior as bash 4.4: not recognized
    let r = exec!("x=HELLO; echo \"${x@L}\"", dialect = Dialect::Bash50);
    assert_eq!(r.stdout(), "\n");
}

#[skuld::test]
fn bash51_allows_transform_lower() {
    // @L works in bash 5.1+
    let r = exec!("x=HELLO; echo ${x@L}", dialect = Dialect::Bash51);
    assert_eq!(r.stdout(), "hello\n");
}

// Ignored tests confirming known TODO items ===========================================================================
//
// Each test asserts the *correct* behavior. They are #[ignore]d because the
// corresponding feature or fix is not yet implemented. Run them with:
//   cargo nextest run --features cli --run-ignored ignored-only
// When a TODO is resolved, remove #[ignore] and the test becomes part of CI.

// heredoc_at_eof (lexer.rs:379): the TODO describes unclean internal lexer
// state, but the observable behavior (parse error) is already correct.
// No failing test can be written for this — the TODO is a code-quality note
// about clearing an internal flag, not a user-visible bug.

#[skuld::test]
#[cfg(unix)]
fn test_dash_big_o_checks_ownership_not_existence() {
    // /etc/passwd exists but is owned by root, not the test user.
    // -O should return false; the bug makes it return true (file exists).
    let r = exec!("[[ -O /etc/passwd ]] && echo yes || echo no", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "no\n", "-O should fail for files not owned by current user");
}

#[skuld::test]
#[cfg(unix)]
fn test_dash_big_g_checks_group_not_existence() {
    // /etc/passwd is typically group-owned by root/wheel, not the test user's group.
    let r = exec!("[[ -G /etc/passwd ]] && echo yes || echo no", dialect = Dialect::Bash);
    assert_eq!(
        r.stdout(),
        "no\n",
        "-G should fail for files not owned by current group"
    );
}

#[skuld::test]
fn test_dash_t_nonexistent_fd_is_false() {
    // FD 99 doesn't exist — -t should return false.
    let r = exec!("[[ -t 99 ]] && echo yes || echo no", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "no\n");
}

#[skuld::test]
fn readonly_no_args_lists_variables() {
    let r = exec!("readonly x=42; readonly", dialect = Dialect::Bash);
    let out = r.stdout();
    assert!(out.contains("x"), "readonly should list readonly variables; got: {out}");
}

#[skuld::test]
fn declare_dash_big_f_lists_function_names() {
    let r = exec!("foo() { echo bar; }; declare -F", dialect = Dialect::Bash);
    let out = r.stdout();
    assert!(out.contains("foo"), "declare -F should list function names; got: {out}");
}

#[skuld::test]
fn declare_dash_f_prints_function_body() {
    let r = exec!("greet() { echo hello; }; declare -f greet", dialect = Dialect::Bash);
    let out = r.stdout();
    assert!(
        out.contains("greet ()"),
        "declare -f should print function header; got: {out}"
    );
    assert!(
        out.contains("echo hello"),
        "declare -f should print function body; got: {out}"
    );
}

#[skuld::test]
fn arith_recursive_variable_expansion() {
    let r = exec!("a=b; b=5; echo $((a))", dialect = Dialect::Bash);
    assert_eq!(r.stdout(), "5\n", "arithmetic should recursively expand variable names");
}

#[skuld::test]
#[cfg(unix)]
fn tilde_user_expansion() {
    // ~root should expand to root's home directory, not stay literal
    let r = exec!("echo ~root");
    let out = r.stdout();
    assert!(!out.starts_with("~root"), "~root should expand; got: {out}");
}

#[skuld::test]
fn tilde_user_expansion_current_user() {
    // Look up the current user's name and verify ~username expands to the
    // same directory that homedir::my_home() returns.
    let expected = homedir::my_home().ok().flatten();
    let username = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" });
    if let (Some(expected_dir), Ok(user)) = (expected, username) {
        let script = format!("echo ~{user}");
        let r = exec!(&script);
        let out = r.stdout();
        let expected_str = expected_dir.to_string_lossy();
        assert_eq!(
            out.trim(),
            expected_str.as_ref(),
            "~{user} should expand to {expected_str}"
        );
    }
    // If USER/USERNAME or homedir is unavailable, skip silently (e.g., containers).
}

#[skuld::test]
fn tilde_nonexistent_user_stays_literal() {
    let r = exec!("echo ~__no_such_user_99__");
    assert_eq!(r.stdout(), "~__no_such_user_99__\n");
}

#[skuld::test]
fn transform_at_big_k_shows_key_value_pairs() {
    let r = exec!(
        r#"declare -A m=([foo]=1 [bar]=2); echo "${m[@]@K}""#,
        dialect = Dialect::Bash
    );
    let out = r.stdout();
    assert!(out.contains("foo"), "@K should produce key=value pairs; got: {out}");
    assert!(out.contains("bar"), "@K should include all keys; got: {out}");
}

#[skuld::test]
fn field_splitting_array_for_loop() {
    let r = exec!(
        r#"a=(x y z); for i in ${a[@]}; do echo $i; done"#,
        dialect = Dialect::Bash
    );
    assert_eq!(
        r.stdout(),
        "x\ny\nz\n",
        "unquoted ${{a[@]}} should field-split into separate words"
    );
}
