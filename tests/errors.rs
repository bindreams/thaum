use thaum::{parse, parse_with, Dialect};

#[test]
fn error_unclosed_if() {
    assert!(parse("if true; then echo yes").is_err());
}

#[test]
fn error_unclosed_while() {
    assert!(parse("while true; do echo loop").is_err());
}

#[test]
fn error_unclosed_for() {
    assert!(parse("for i in a b; do echo $i").is_err());
}

#[test]
fn error_unclosed_case() {
    assert!(parse("case x in\na) echo a;;").is_err());
}

#[test]
fn error_unclosed_subshell() {
    assert!(parse("(echo hello").is_err());
}

#[test]
fn error_unclosed_brace_group() {
    assert!(parse("{ echo hello").is_err());
}

#[test]
fn error_unterminated_quote() {
    assert!(parse("echo 'unterminated").is_err());
}

#[test]
fn error_unterminated_double_quote() {
    assert!(parse("echo \"unterminated").is_err());
}

#[test]
fn error_leading_and_if() {
    assert!(parse("&&").is_err());
}

#[test]
fn error_leading_or_if() {
    assert!(parse("||").is_err());
}

#[test]
fn error_leading_pipe() {
    assert!(parse("| cmd").is_err());
}

#[test]
fn error_leading_semicolon() {
    assert!(parse("; cmd").is_err());
}

#[test]
fn error_trailing_and_if() {
    assert!(parse("cmd &&").is_err());
}

#[test]
fn error_trailing_or_if() {
    assert!(parse("cmd ||").is_err());
}

#[test]
fn error_trailing_pipe() {
    assert!(parse("cmd |").is_err());
}

#[test]
fn error_redirect_no_target() {
    assert!(parse("cmd >").is_err());
}

#[test]
fn error_append_no_target() {
    assert!(parse("cmd >>").is_err());
}

#[test]
fn error_input_no_target() {
    assert!(parse("cmd <").is_err());
}

#[test]
fn error_unmatched_rparen() {
    assert!(parse(")").is_err());
}

#[test]
fn error_unmatched_rbrace() {
    assert!(parse("}").is_err());
}

#[test]
fn error_stray_fi() {
    assert!(parse("fi").is_err());
}

#[test]
fn error_stray_done() {
    assert!(parse("done").is_err());
}

#[test]
fn error_stray_esac() {
    assert!(parse("esac").is_err());
}

#[test]
fn error_double_and_or() {
    assert!(parse("cmd && || b").is_err());
}

#[test]
fn error_if_empty_condition() {
    assert!(parse("if then echo yes; fi").is_err());
}

#[test]
fn error_if_empty_body() {
    assert!(parse("if true; then fi").is_err());
}

#[test]
fn error_while_empty_condition() {
    assert!(parse("while do echo yes; done").is_err());
}

#[test]
fn error_while_empty_body() {
    assert!(parse("while true; do done").is_err());
}

#[test]
fn error_for_empty_body() {
    assert!(parse("for i in a b; do done").is_err());
}

#[test]
fn error_lone_redirect() {
    assert!(parse(">").is_err());
}

#[test]
fn error_unterminated_dollar_paren() {
    assert!(parse("echo $(echo test").is_err());
}

#[test]
fn error_unterminated_dollar_paren_with_semi() {
    assert!(parse("echo hello | grep $(echo test ;").is_err());
}

#[test]
fn error_unterminated_backtick() {
    assert!(parse("echo `echo test").is_err());
}

// Newline directly after keyword (grammar requires whitespace, not newline)

#[test]
fn error_for_newline_before_name() {
    assert!(parse("for\nx in a b; do echo $x; done").is_err());
}

#[test]
fn error_case_newline_before_word() {
    assert!(parse("case\nx in a) ;; esac").is_err());
}

#[test]
fn error_function_newline_before_name() {
    assert!(parse_with("function\nfoo { :; }", Dialect::Bash).is_err());
}

#[test]
fn error_select_newline_before_name() {
    assert!(parse_with("select\nx in a b; do echo $x; done", Dialect::Bash).is_err());
}

#[test]
fn error_coproc_newline_before_command() {
    assert!(parse_with("coproc\ncat", Dialect::Bash).is_err());
}
