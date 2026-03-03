use std::path::Path;

use thaum::Dialect;

use crate::*;

// Bash alias expansion ------------------------------------------------------------------------------------------------

#[testutil::test]
fn alias_basic() {
    let (out, status) = bash_exec_ok("shopt -s expand_aliases\nalias hi='echo hello'\nhi");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn alias_requires_shopt() {
    // Without shopt -s expand_aliases, aliases are defined but not expanded
    let (_, status) = bash_exec_ok("alias hi='echo hello'\nhi");
    assert_ne!(status, 0);
}

#[testutil::test]
fn alias_same_line_not_expanded() {
    // alias e=echo; e one — same line, e is NOT expanded (parsed before defined)
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias e=echo; e one");
    assert_ne!(status, 0);
}

#[testutil::test]
fn alias_cross_line_expanded() {
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias e=echo\ne hello");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn alias_semicolon_then_newline() {
    // alias a="echo";  ← trailing semicolon, then newline → next line sees alias
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias a=echo;\na hello");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn alias_unalias() {
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias e=echo\nunalias e\ne hello");
    assert_ne!(status, 0);
}

#[testutil::test]
fn alias_unalias_same_line() {
    // alias + unalias on one line; next line sees no alias
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias a=echo; unalias a\na hello");
    assert_ne!(status, 0);
}

#[testutil::test]
fn alias_recursive() {
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias hi='e_ hello'\nalias e_='echo __'\nhi");
    assert_eq!(out, "__ hello\n");
}

#[testutil::test]
fn alias_trailing_space() {
    // Alias ending with space → next word also alias-expanded
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias hi='echo '\nalias w='hello'\nhi w");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn alias_quoted_not_expanded() {
    // Quoted command name must NOT trigger alias expansion
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias hi='echo hello'\n'hi'");
    assert_ne!(status, 0);
}

#[testutil::test]
fn alias_list() {
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias e=echo\nalias");
    assert!(out.contains("alias e='echo'") || out.contains("alias e=echo"));
}

#[testutil::test]
fn alias_redefine_then_unalias() {
    // Line 2: alias a="touch"  → defines a=touch
    // Line 3: alias a="echo"; unalias a  → redefines then removes
    // Line 4: a hello  → not found (unalias took effect)
    let (_, status) = bash_exec_ok("shopt -s expand_aliases\nalias a=touch\nalias a=echo; unalias a\na hello");
    assert_ne!(status, 0);
}

#[testutil::test]
fn alias_snapshot_uses_previous_line() {
    // Line 2: alias a="echo"  → defines a=echo
    // Line 3: alias a="touch"; a hello; unalias a
    //   → snapshot for line 3 has a=echo (from before line 3 executed)
    //   → so "a hello" expands to "echo hello" (not "touch hello")
    //   → then alias a is redefined to touch, then unaliased — both during execution
    // Line 4: a hello  → not found (unalias from line 3 took effect)
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias a=echo\nalias a=touch; a hello; unalias a");
    assert_eq!(out, "hello\n");
}

#[cfg(unix)]
#[testutil::test]
fn alias_snapshot_touch_file(#[fixture(temp_dir)] dir: &Path) {
    // Line 2: alias a="touch"
    // Line 3: alias a="echo"; a hello; unalias a
    //   → snapshot for line 3 has a=touch (from line 2)
    //   → "a hello" expands to "touch hello" (creates file)
    // Line 4: a hello  → not found
    let file = dir.join("hello");

    let script = format!(
        "shopt -s expand_aliases\nalias a=touch\ncd {}; alias a=echo; a hello; unalias a",
        dir.to_string_lossy()
    );
    let (_, _) = bash_exec_ok(&script);
    assert!(file.exists(), "touch hello should have created the file");
}

// Alias funkiness levels (see CONTRIBUTING.md) ========================================================================
//
// Levels 1–4 must work. Level 5 (partial compound syntax) is unsupported by design.

#[testutil::test]
fn alias_funkiness_level2_multiple_words() {
    // Level 2: alias value contains command + flags (multiple words)
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias greet='echo -n hello'\ngreet");
    assert_eq!(out, "hello");
}

#[testutil::test]
fn alias_funkiness_level3a_redirect_in_value(#[fixture(temp_dir)] dir: &Path) {
    // Level 3a: alias value contains a redirect — verify the file is created.
    // We check the file on disk instead of captured stdout because pipeline/redirect
    // I/O goes through real file descriptors, not CapturedIo.
    let file = dir.join("out.txt");

    let f = file.to_string_lossy().replace('\\', "/");
    let script = format!("shopt -s expand_aliases\nalias w='echo hello >'\nw {f}");
    bash_exec_ok(&script);
    let contents = std::fs::read_to_string(&file).expect("redirect should have created file");
    assert_eq!(contents.trim(), "hello");
}

#[testutil::test]
fn alias_funkiness_level3b_command_sub_in_value() {
    // Level 3b: alias value contains $() command substitution
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias greet='echo $(echo hi)'\ngreet");
    assert_eq!(out, "hi\n");
}

#[cfg(unix)]
#[testutil::test]
fn alias_funkiness_level4a_pipe_in_value() {
    // Level 4: alias value contains a pipe (creates a pipeline).
    // The pipeline result feeds into a subsequent command via ; to verify it ran.
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias both='echo hello | cat; echo'\nboth done");
    // After expansion: `echo hello | cat; echo done`
    // Pipeline output goes through real FDs (not CapturedIo) but the second
    // command `echo done` is a simple builtin that IS captured.
    assert!(out.contains("done"), "commands after pipe should run; got: {out}");
}

#[testutil::test]
fn alias_funkiness_level4b_semicolons_in_value() {
    // Level 4: alias value contains ; (splits into multiple commands)
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias both='echo one; echo'\nboth two");
    assert_eq!(out, "one\ntwo\n");
}

#[testutil::test]
fn alias_funkiness_level4c_and_chain_in_value() {
    // Level 4: alias value contains && (and-chain)
    let (out, _) = bash_exec_ok("shopt -s expand_aliases\nalias chk='true && echo'\nchk ok");
    assert_eq!(out, "ok\n");
}

// Subshell execution --------------------------------------------------------------------------------------------------

#[testutil::test]
fn subshell_basic() {
    let (out, status) = exec_ok("(echo hello)");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn subshell_exit_status() {
    let (out, _) = exec_ok("(exit 42); echo $?");
    assert_eq!(out, "42\n");
}

#[testutil::test]
fn subshell_variable_isolation() {
    let (out, _) = exec_ok("x=1; (x=2); echo $x");
    assert_eq!(out, "1\n");
}

#[testutil::test]
fn subshell_inherits_vars() {
    let (out, _) = exec_ok("x=hello; (echo $x)");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn subshell_inherits_functions() {
    let (out, _) = exec_ok("f() { echo hi; }; (f)");
    assert_eq!(out, "hi\n");
}

#[testutil::test]
fn subshell_nested() {
    let (out, _) = exec_ok("((echo inner))");
    assert_eq!(out, "inner\n");
}

#[testutil::test]
fn subshell_with_redirect(#[fixture(temp_dir)] dir: &Path) {
    // Redirect inside the subshell (not on the compound command).
    let file = dir.join("out.txt");

    let script = format!("(echo hello > {})", file.to_string_lossy().replace('\\', "/"));
    let (out, status) = exec_ok(&script);
    assert_eq!(status, 0);
    assert_eq!(out, ""); // stdout went to file inside subshell
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

// Bash [[ ]] conditional ----------------------------------------------------------------------------------------------

#[testutil::test]
fn bash_cond_string_equals() {
    let (_, status) = bash_exec_ok("[[ hello == hello ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_string_not_equals() {
    let (_, status) = bash_exec_ok("[[ a != b ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_false() {
    let (_, status) = bash_exec_ok("[[ a == b ]]");
    assert_eq!(status, 1);
}

#[testutil::test]
fn bash_cond_string_empty() {
    let (_, status) = bash_exec_ok("[[ -z '' ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_string_nonempty() {
    let (_, status) = bash_exec_ok("[[ -n hello ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_file_exists() {
    let (_, status) = bash_exec_ok("[[ -e /tmp ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_file_is_dir() {
    let (_, status) = bash_exec_ok("[[ -d /tmp ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_file_not_exists() {
    let (_, status) = bash_exec_ok("[[ -e /nonexistent_path_xyz ]]");
    assert_eq!(status, 1);
}

#[testutil::test]
fn bash_cond_int_eq() {
    let (_, status) = bash_exec_ok("[[ 42 -eq 42 ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_int_lt() {
    let (_, status) = bash_exec_ok("[[ 1 -lt 2 ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_and() {
    let (_, status) = bash_exec_ok("[[ -n a && -n b ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_or() {
    let (_, status) = bash_exec_ok("[[ -z '' || -n b ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_not() {
    let (_, status) = bash_exec_ok("[[ ! -z hello ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_variable() {
    let (_, status) = bash_exec_ok("x=hi; [[ -n $x ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_regex() {
    let (_, status) = bash_exec_ok("[[ abc123 =~ [0-9]+ ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_regex_no_match() {
    let (_, status) = bash_exec_ok("[[ abcdef =~ [0-9]+ ]]");
    assert_eq!(status, 1);
}

#[testutil::test]
fn bash_cond_lexical_lt() {
    let (_, status) = bash_exec_ok("[[ apple < banana ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_var_set() {
    let (_, status) = bash_exec_ok("x=1; [[ -v x ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_var_unset() {
    let (_, status) = bash_exec_ok("[[ -v nonexistent_var ]]");
    assert_eq!(status, 1);
}

#[testutil::test]
fn bash_cond_in_if() {
    let (out, _) = bash_exec_ok("if [[ 1 -eq 1 ]]; then echo yes; fi");
    assert_eq!(out, "yes\n");
}

#[testutil::test]
fn bash_cond_bare_word() {
    // Bare non-empty word is true (implicit -n)
    let (_, status) = bash_exec_ok("[[ hello ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn bash_cond_bare_empty() {
    // Empty string is false
    let (_, status) = bash_exec_ok("[[ '' ]]");
    assert_eq!(status, 1);
}

// set -x (xtrace) -----------------------------------------------------------------------------------------------------

#[testutil::test]
fn set_x_basic() {
    // xtrace goes to stderr; stdout should only contain the echo output
    let (out, status) = exec_ok("set -x; echo hello");
    assert_eq!(status, 0);
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn set_x_off() {
    let (out, _) = exec_ok("set -x; set +x; echo hello");
    assert_eq!(out, "hello\n");
}

// set -u (nounset) ----------------------------------------------------------------------------------------------------

#[testutil::test]
fn set_u_unset_var() {
    let status = exec_result("set -u; echo $nonexistent_xyz");
    assert_ne!(status, 0);
}

#[testutil::test]
fn set_u_set_var() {
    let (out, _) = exec_ok("set -u; x=hi; echo $x");
    assert_eq!(out, "hi\n");
}

#[testutil::test]
fn set_u_default() {
    let (out, _) = exec_ok("set -u; echo ${nonexistent_xyz:-fallback}");
    assert_eq!(out, "fallback\n");
}

#[testutil::test]
fn set_u_special() {
    let (out, _) = exec_ok("set -u; echo $?");
    assert_eq!(out, "0\n");
}

#[testutil::test]
fn set_u_off() {
    let (out, _) = exec_ok("set -u; set +u; echo ${nonexistent_xyz}done");
    assert_eq!(out, "done\n");
}

// set -e (errexit) ----------------------------------------------------------------------------------------------------

#[testutil::test]
fn set_e_basic() {
    // false triggers errexit — "nope" is never printed
    let (out, status) = exec_ok("set -e; false; echo nope");
    assert_eq!(out, "");
    assert_ne!(status, 0);
}

#[testutil::test]
fn set_e_if_condition() {
    // false in if condition does NOT trigger errexit
    let (out, _) = exec_ok("set -e; if false; then echo then; fi; echo ok");
    assert_eq!(out, "ok\n");
}

#[testutil::test]
fn set_e_and_chain() {
    // false on left side of && does NOT trigger errexit
    let (out, _) = exec_ok("set -e; false && true; echo ok");
    assert_eq!(out, "ok\n");
}

#[testutil::test]
fn set_e_or_chain() {
    // false on left side of || does NOT trigger errexit
    let (out, _) = exec_ok("set -e; false || true; echo ok");
    assert_eq!(out, "ok\n");
}

#[testutil::test]
fn set_e_not() {
    // ! false (negation) does NOT trigger errexit
    let (out, _) = exec_ok("set -e; ! false; echo ok");
    assert_eq!(out, "ok\n");
}

#[testutil::test]
fn set_e_off() {
    // set +e disables errexit
    let (out, _) = exec_ok("set -e; set +e; false; echo ok");
    assert_eq!(out, "ok\n");
}

// Case modification operators (${var^}, ${var^^}, ${var,}, ${var,,}) ---------------------------------------------------

#[testutil::test]
fn case_mod_upper_first() {
    let (out, _) = bash_exec_ok("x=hello; echo ${x^}");
    assert_eq!(out, "Hello\n");
}

#[testutil::test]
fn case_mod_upper_all() {
    let (out, _) = bash_exec_ok("x=hello; echo ${x^^}");
    assert_eq!(out, "HELLO\n");
}

#[testutil::test]
fn case_mod_lower_first() {
    let (out, _) = bash_exec_ok("x=HELLO; echo ${x,}");
    assert_eq!(out, "hELLO\n");
}

#[testutil::test]
fn case_mod_lower_all() {
    let (out, _) = bash_exec_ok("x=HELLO; echo ${x,,}");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn case_mod_unicode() {
    let (out, _) = bash_exec_ok("x=café; echo ${x^^}");
    assert_eq!(out, "CAFÉ\n");
}

#[testutil::test]
fn case_mod_empty() {
    let (out, _) = bash_exec_ok("x=''; echo \"${x^^}\"");
    assert_eq!(out, "\n");
}

#[testutil::test]
fn case_mod_unset() {
    let (out, _) = bash_exec_ok("echo \"${unset_var^^}\"");
    assert_eq!(out, "\n");
}

// POSIX character classes in case =================================================================================

#[testutil::test]
fn case_char_class_upper() {
    let (out, _) = bash_exec_ok("case A in [[:upper:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[testutil::test]
fn case_char_class_lower() {
    let (out, _) = bash_exec_ok("case a in [[:lower:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[testutil::test]
fn case_char_class_digit() {
    let (out, _) = bash_exec_ok("case 5 in [[:digit:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[testutil::test]
fn case_char_class_space() {
    let (out, _) = exec_ok("case ' ' in [[:space:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[testutil::test]
fn case_char_class_alpha_negated() {
    let (out, _) = bash_exec_ok("case 5 in [![:alpha:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[testutil::test]
fn case_char_class_mixed_bracket() {
    // Class + literal in same bracket
    let (out, _) = bash_exec_ok("case _ in [[:alpha:]_]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

#[testutil::test]
fn case_char_class_alnum_with_star() {
    let (out, _) = bash_exec_ok("case hello123 in [[:alnum:]]*) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

// Character classes in parameter expansion ========================================================================

#[testutil::test]
fn trim_char_class_alpha_prefix() {
    let (out, _) = bash_exec_ok("x=hello123; echo ${x##[[:alpha:]]*}");
    assert_eq!(out, "\n");
}

#[testutil::test]
fn trim_char_class_digit_suffix() {
    let (out, _) = bash_exec_ok("x=hello123; echo ${x%%[[:digit:]]*}");
    assert_eq!(out, "hello\n");
}

// Regex =~ with character classes =================================================================================

#[testutil::test]
fn regex_char_class_digit() {
    let (_, status) = bash_exec_ok("[[ abc123 =~ [[:digit:]]+ ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn regex_char_class_alpha() {
    let (_, status) = bash_exec_ok("[[ hello =~ ^[[:alpha:]]+$ ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn regex_char_class_space() {
    let (_, status) = bash_exec_ok("[[ 'hello world' =~ [[:space:]] ]]");
    assert_eq!(status, 0);
}

#[testutil::test]
fn regex_char_class_upper() {
    let (_, status) = bash_exec_ok("[[ Hello =~ ^[[:upper:]] ]]");
    assert_eq!(status, 0);
}

// Locale sensitivity of character classes =========================================================================

#[testutil::test]
fn case_char_class_upper_accent_c_locale() {
    // In C locale, É is NOT [[:upper:]]
    let (out, _) = bash_exec_ok("LC_CTYPE=C; case É in [[:upper:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "n\n");
}

#[testutil::test]
fn case_char_class_upper_accent_utf8_locale() {
    // In UTF-8 locale, É IS [[:upper:]]
    let (out, _) = bash_exec_ok("LC_CTYPE=en_US.UTF-8; case É in [[:upper:]]) echo y;; *) echo n;; esac");
    assert_eq!(out, "y\n");
}

// Locale translation ($"...") =========================================================================================

#[testutil::test]
fn locale_quoted_no_domain() {
    // Without TEXTDOMAIN, $"..." just expands like double quotes
    let (out, _) = bash_exec_ok("echo $\"hello world\"");
    assert_eq!(out, "hello world\n");
}

#[testutil::test]
fn locale_quoted_with_variable_no_domain() {
    // $"..." expands variables even without translation
    let (out, _) = bash_exec_ok("x=test; echo $\"hello $x\"");
    assert_eq!(out, "hello test\n");
}

#[testutil::test]
fn locale_quoted_basic_translation() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"hello world\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "hallo welt\n");
}

#[testutil::test]
fn locale_quoted_with_variable_translation() {
    let script = format!(
        "USER=Claude\nTEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"hello $USER\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "hallo Claude\n");
}

#[testutil::test]
fn locale_quoted_missing_msgid() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=de\necho $\"not in catalog\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "not in catalog\n");
}

#[testutil::test]
fn locale_quoted_c_locale_no_translation() {
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLC_MESSAGES=C\necho $\"hello world\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "hello world\n");
}

#[testutil::test]
fn locale_quoted_empty_string() {
    let (out, _) = bash_exec_ok("echo $\"\"");
    assert_eq!(out, "\n");
}

#[testutil::test]
fn locale_quoted_fallback_locale() {
    // LANG=de_DE.UTF-8 with .mo only in de/ directory -- should fall back
    let script = format!(
        "TEXTDOMAIN=testdomain\nTEXTDOMAINDIR={}\nLANG=de_DE.UTF-8\necho $\"goodbye\"",
        fixture_dir()
    );
    let (out, _) = bash_exec_ok(&script);
    assert_eq!(out, "auf wiedersehen\n");
}

// Parameter transformation @Q/@a/@A -----------------------------------------------------------------------------------

#[testutil::test]
fn transform_quote_simple() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@Q}\"");
    assert_eq!(out, "'hello'\n");
}

#[testutil::test]
fn transform_attrs_plain() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@a}\"");
    assert_eq!(out, "\n");
}

#[testutil::test]
fn transform_attrs_integer() {
    let (out, _) = bash_exec_ok("declare -i n=42; echo \"${n@a}\"");
    assert_eq!(out, "i\n");
}

#[testutil::test]
fn transform_attrs_exported_readonly() {
    let (out, _) = bash_exec_ok("declare -rx e=test; echo \"${e@a}\"");
    assert_eq!(out, "rx\n");
}

#[testutil::test]
fn transform_attrs_array() {
    let (out, _) = bash_exec_ok("declare -a a; a=(1 2); echo \"${a@a}\"");
    assert_eq!(out, "a\n");
}

#[testutil::test]
fn transform_attrs_assoc() {
    let (out, _) = bash_exec_ok("declare -A m=([k]=v); echo \"${m@a}\"");
    assert_eq!(out, "A\n");
}

#[testutil::test]
fn transform_assign_scalar() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@A}\"");
    assert_eq!(out, "x='hello'\n");
}

#[testutil::test]
fn transform_assign_integer() {
    let (out, _) = bash_exec_ok("declare -i n=42; echo \"${n@A}\"");
    assert_eq!(out, "declare -i n='42'\n");
}

#[testutil::test]
fn transform_lower() {
    let (out, _) = bash_exec_ok("x=HELLO; echo \"${x@L}\"");
    assert_eq!(out, "hello\n");
}

#[testutil::test]
fn transform_upper() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@U}\"");
    assert_eq!(out, "HELLO\n");
}

#[testutil::test]
fn transform_capitalize() {
    let (out, _) = bash_exec_ok("x=hello; echo \"${x@u}\"");
    assert_eq!(out, "Hello\n");
}

// Indirect expansion ${!var[@]} ---------------------------------------------------------------------------------------

#[testutil::test]
fn indirect_array_keys() {
    let (out, _) = bash_exec_ok("a=(x y z); echo ${!a[@]}");
    assert_eq!(out, "0 1 2\n");
}

#[testutil::test]
fn indirect_assoc_keys() {
    // Assoc array keys are unordered, so just check we get both
    let (out, _) = bash_exec_ok("declare -A m; m[k]=v; m[j]=w; echo ${!m[@]}");
    let keys: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"k"));
    assert!(keys.contains(&"j"));
}

// Versioned dialect tests -----------------------------------------------------------------------------------------

#[testutil::test]
fn bash_is_bash51() {
    // Dialect::Bash and Dialect::Bash51 produce identical options
    assert_eq!(Dialect::Bash.options(), Dialect::Bash51.options());
}

#[testutil::test]
fn bash44_has_array_empty_element_bug() {
    // In bash 4.4, ${a[@]:+foo} on array with empty element returns "foo" (bug)
    let (out, _) = dialect_exec_ok("a=(''); echo \"${a[@]:+foo}\"", Dialect::Bash44);
    assert_eq!(out, "foo\n");
}

#[testutil::test]
fn bash50_fixes_array_empty_element_bug() {
    // In bash 5.0+, ${a[@]:+foo} on array with empty element returns "" (fixed)
    let (out, _) = dialect_exec_ok("a=(''); echo \"${a[@]:+foo}\"", Dialect::Bash50);
    assert_eq!(out, "\n");
}

#[testutil::test]
fn bash44_rejects_transform_lower() {
    // @L is bash 5.1+ — in bash 4.4, the parser does not recognize @L as a
    // transform, so `x@L` is treated as a variable name containing `@` which
    // expands to empty (no bad-substitution error at parse time, but the
    // transform is not applied).
    let (out, _) = dialect_exec_ok("x=HELLO; echo \"${x@L}\"", Dialect::Bash44);
    // Without the transform, `x@L` is an undefined variable → empty
    assert_eq!(out, "\n");
}

#[testutil::test]
fn bash50_rejects_transform_lower() {
    // @L is bash 5.1+ — same behavior as bash 4.4: not recognized
    let (out, _) = dialect_exec_ok("x=HELLO; echo \"${x@L}\"", Dialect::Bash50);
    assert_eq!(out, "\n");
}

#[testutil::test]
fn bash51_allows_transform_lower() {
    // @L works in bash 5.1+
    let (out, _) = dialect_exec_ok("x=HELLO; echo ${x@L}", Dialect::Bash51);
    assert_eq!(out, "hello\n");
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

#[testutil::test]
#[cfg(unix)]
fn test_dash_big_o_checks_ownership_not_existence() {
    // /etc/passwd exists but is owned by root, not the test user.
    // -O should return false; the bug makes it return true (file exists).
    let (out, _) = bash_exec_ok("[[ -O /etc/passwd ]] && echo yes || echo no");
    assert_eq!(out, "no\n", "-O should fail for files not owned by current user");
}

#[testutil::test]
#[cfg(unix)]
fn test_dash_big_g_checks_group_not_existence() {
    // /etc/passwd is typically group-owned by root/wheel, not the test user's group.
    let (out, _) = bash_exec_ok("[[ -G /etc/passwd ]] && echo yes || echo no");
    assert_eq!(out, "no\n", "-G should fail for files not owned by current group");
}

#[testutil::test]
fn test_dash_t_nonexistent_fd_is_false() {
    // FD 99 doesn't exist — -t should return false.
    let (out, _) = bash_exec_ok("[[ -t 99 ]] && echo yes || echo no");
    assert_eq!(out, "no\n");
}

#[testutil::test]
fn readonly_no_args_lists_variables() {
    let (out, _) = bash_exec_ok("readonly x=42; readonly");
    assert!(out.contains("x"), "readonly should list readonly variables; got: {out}");
}

#[testutil::test]
fn declare_dash_big_f_lists_function_names() {
    let (out, _) = bash_exec_ok("foo() { echo bar; }; declare -F");
    assert!(out.contains("foo"), "declare -F should list function names; got: {out}");
}

#[testutil::test]
fn declare_dash_f_prints_function_body() {
    let (out, _) = bash_exec_ok("greet() { echo hello; }; declare -f greet");
    assert!(
        out.contains("greet ()"),
        "declare -f should print function header; got: {out}"
    );
    assert!(
        out.contains("echo hello"),
        "declare -f should print function body; got: {out}"
    );
}

#[testutil::test]
fn arith_recursive_variable_expansion() {
    let (out, _) = bash_exec_ok("a=b; b=5; echo $((a))");
    assert_eq!(out, "5\n", "arithmetic should recursively expand variable names");
}

#[testutil::test]
#[cfg(unix)]
fn tilde_user_expansion() {
    // ~root should expand to root's home directory, not stay literal
    let (out, _) = exec_ok("echo ~root");
    assert!(!out.starts_with("~root"), "~root should expand; got: {out}");
}

#[testutil::test]
fn tilde_user_expansion_current_user() {
    // Look up the current user's name and verify ~username expands to the
    // same directory that homedir::my_home() returns.
    let expected = homedir::my_home().ok().flatten();
    let username = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" });
    if let (Some(expected_dir), Ok(user)) = (expected, username) {
        let script = format!("echo ~{user}");
        let (out, _) = exec_ok(&script);
        let expected_str = expected_dir.to_string_lossy();
        assert_eq!(
            out.trim(),
            expected_str.as_ref(),
            "~{user} should expand to {expected_str}"
        );
    }
    // If USER/USERNAME or homedir is unavailable, skip silently (e.g., containers).
}

#[testutil::test]
fn tilde_nonexistent_user_stays_literal() {
    let (out, _) = exec_ok("echo ~__no_such_user_99__");
    assert_eq!(out, "~__no_such_user_99__\n");
}

#[testutil::test]
fn transform_at_big_k_shows_key_value_pairs() {
    let (out, _) = bash_exec_ok(r#"declare -A m=([foo]=1 [bar]=2); echo "${m[@]@K}""#);
    assert!(out.contains("foo"), "@K should produce key=value pairs; got: {out}");
    assert!(out.contains("bar"), "@K should include all keys; got: {out}");
}

#[testutil::test]
fn field_splitting_array_for_loop() {
    let (out, _) = bash_exec_ok(r#"a=(x y z); for i in ${a[@]}; do echo $i; done"#);
    assert_eq!(
        out, "x\ny\nz\n",
        "unquoted ${{a[@]}} should field-split into separate words"
    );
}
