use super::*;

fn parse(raw: &str) -> Vec<Fragment> {
    parse_word(raw, Span::new(0, raw.len()), &ParseOptions::default()).parts
}

// === Literals ===

#[test]
fn plain_word() {
    assert_eq!(parse("hello"), vec![Fragment::Literal("hello".into())]);
}

#[test]
fn word_with_slash() {
    assert_eq!(
        parse("/usr/bin/ls"),
        vec![Fragment::Literal("/usr/bin/ls".into())]
    );
}

// === Single quotes ===

#[test]
fn single_quoted() {
    assert_eq!(
        parse("'hello world'"),
        vec![Fragment::SingleQuoted("hello world".into())]
    );
}

#[test]
fn single_quote_preserves_special() {
    assert_eq!(parse("'$var'"), vec![Fragment::SingleQuoted("$var".into())]);
}

// === Double quotes ===

#[test]
fn double_quoted_literal() {
    assert_eq!(
        parse("\"hello\""),
        vec![Fragment::DoubleQuoted(vec![Fragment::Literal(
            "hello".into()
        )])]
    );
}

#[test]
fn double_quoted_with_expansion() {
    assert_eq!(
        parse("\"hello $name\""),
        vec![Fragment::DoubleQuoted(vec![
            Fragment::Literal("hello ".into()),
            Fragment::Parameter(ParameterExpansion::Simple("name".into())),
        ])]
    );
}

#[test]
fn double_quoted_with_command_subst() {
    let parts = parse("\"$(echo hi)\"");
    assert_eq!(parts.len(), 1);
    if let Fragment::DoubleQuoted(inner) = &parts[0] {
        assert_eq!(inner.len(), 1);
        assert!(matches!(&inner[0], Fragment::CommandSubstitution(_)));
    } else {
        panic!("expected DoubleQuoted");
    }
}

// === Backslash escapes ===

#[test]
fn backslash_escape_outside_quotes() {
    assert_eq!(
        parse("hello\\ world"),
        vec![Fragment::Literal("hello world".into())]
    );
}

// === Parameter expansion ===

#[test]
fn simple_param() {
    assert_eq!(
        parse("$var"),
        vec![Fragment::Parameter(ParameterExpansion::Simple(
            "var".into()
        ))]
    );
}

#[test]
fn param_with_surrounding_text() {
    assert_eq!(
        parse("pre$var-post"),
        vec![
            Fragment::Literal("pre".into()),
            Fragment::Parameter(ParameterExpansion::Simple("var".into())),
            Fragment::Literal("-post".into()),
        ]
    );
}

#[test]
fn special_params() {
    assert_eq!(
        parse("$@"),
        vec![Fragment::Parameter(ParameterExpansion::Simple("@".into()))]
    );
    assert_eq!(
        parse("$?"),
        vec![Fragment::Parameter(ParameterExpansion::Simple("?".into()))]
    );
    assert_eq!(
        parse("$$"),
        vec![Fragment::Parameter(ParameterExpansion::Simple("$".into()))]
    );
    assert_eq!(
        parse("$!"),
        vec![Fragment::Parameter(ParameterExpansion::Simple("!".into()))]
    );
    assert_eq!(
        parse("$#"),
        vec![Fragment::Parameter(ParameterExpansion::Simple("#".into()))]
    );
}

#[test]
fn positional_param() {
    assert_eq!(
        parse("$1"),
        vec![Fragment::Parameter(ParameterExpansion::Simple("1".into()))]
    );
}

// === Brace parameter expansion ===

#[test]
fn brace_simple() {
    assert_eq!(
        parse("${var}"),
        vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "var".into(),
            operator: None,
            argument: None,
        })]
    );
}

#[test]
fn brace_default() {
    let parts = parse("${var:-default}");
    assert_eq!(
        parts,
        vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "var".into(),
            operator: Some(ParamOp::Default),
            argument: Some(Box::new(Word {
                parts: vec![Fragment::Literal("default".into())],
                span: Span::empty(0),
            })),
        })]
    );
}

#[test]
fn brace_length() {
    assert_eq!(
        parse("${#var}"),
        vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "var".into(),
            operator: Some(ParamOp::Length),
            argument: None,
        })]
    );
}

#[test]
fn brace_trim_suffix() {
    let parts = parse("${var%pattern}");
    assert_eq!(
        parts,
        vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "var".into(),
            operator: Some(ParamOp::TrimSmallSuffix),
            argument: Some(Box::new(Word {
                parts: vec![Fragment::Literal("pattern".into())],
                span: Span::empty(0),
            })),
        })]
    );
}

#[test]
fn brace_trim_prefix_large() {
    let parts = parse("${var##pattern}");
    assert_eq!(
        parts,
        vec![Fragment::Parameter(ParameterExpansion::Complex {
            name: "var".into(),
            operator: Some(ParamOp::TrimLargePrefix),
            argument: Some(Box::new(Word {
                parts: vec![Fragment::Literal("pattern".into())],
                span: Span::empty(0),
            })),
        })]
    );
}

// === Command substitution ===

#[test]
fn dollar_paren_command_subst() {
    let parts = parse("$(echo hello)");
    assert_eq!(parts.len(), 1);
    if let Fragment::CommandSubstitution(stmts) = &parts[0] {
        assert_eq!(stmts.len(), 1);
        if let Expression::Command(cmd) = &stmts[0].expression {
            assert_eq!(cmd.arguments.len(), 2);
        } else {
            panic!("expected Command");
        }
    } else {
        panic!("expected CommandSubstitution");
    }
}

#[test]
fn backtick_command_subst() {
    let parts = parse("`echo hello`");
    assert_eq!(parts.len(), 1);
    if let Fragment::CommandSubstitution(stmts) = &parts[0] {
        assert_eq!(stmts.len(), 1);
        if let Expression::Command(cmd) = &stmts[0].expression {
            assert_eq!(cmd.arguments.len(), 2);
        } else {
            panic!("expected Command");
        }
    } else {
        panic!("expected CommandSubstitution");
    }
}

#[test]
fn nested_command_subst() {
    // $(echo $(cat file)) — outer is a command with two words,
    // second word contains a nested CommandSubstitution
    let parts = parse("$(echo $(cat file))");
    assert_eq!(parts.len(), 1);
    if let Fragment::CommandSubstitution(stmts) = &parts[0] {
        assert_eq!(stmts.len(), 1);
        if let Expression::Command(cmd) = &stmts[0].expression {
            assert_eq!(cmd.arguments.len(), 2);
            // Second argument should be a Word containing a nested command substitution
            if let Argument::Word(w) = &cmd.arguments[1] {
                assert!(w
                    .parts
                    .iter()
                    .any(|p| matches!(p, Fragment::CommandSubstitution(_))));
            } else {
                panic!("expected Word argument");
            }
        } else {
            panic!("expected Command");
        }
    } else {
        panic!("expected CommandSubstitution");
    }
}

// === Arithmetic expansion ===

#[test]
fn arithmetic_expansion() {
    assert_eq!(
        parse("$((1 + 2))"),
        vec![Fragment::ArithmeticExpansion(ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(1)),
            op: ArithBinaryOp::Add,
            right: Box::new(ArithExpr::Number(2)),
        })]
    );
}

// === Glob patterns ===

#[test]
fn glob_star() {
    assert_eq!(
        parse("*.txt"),
        vec![
            Fragment::Glob(GlobChar::Star),
            Fragment::Literal(".txt".into()),
        ]
    );
}

#[test]
fn glob_question() {
    assert_eq!(
        parse("file?.txt"),
        vec![
            Fragment::Literal("file".into()),
            Fragment::Glob(GlobChar::Question),
            Fragment::Literal(".txt".into()),
        ]
    );
}

#[test]
fn glob_bracket() {
    let parts = parse("[abc]");
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], Fragment::Glob(GlobChar::BracketOpen));
    assert_eq!(parts[1], Fragment::Literal("abc]".into()));
}

// === Tilde expansion ===

#[test]
fn tilde_alone() {
    assert_eq!(parse("~"), vec![Fragment::TildePrefix("".into())]);
}

#[test]
fn tilde_with_user() {
    assert_eq!(
        parse("~user/file"),
        vec![
            Fragment::TildePrefix("user".into()),
            Fragment::Literal("/file".into()),
        ]
    );
}

#[test]
fn tilde_with_path() {
    assert_eq!(
        parse("~/bin"),
        vec![
            Fragment::TildePrefix("".into()),
            Fragment::Literal("/bin".into()),
        ]
    );
}

// === Mixed ===

#[test]
fn mixed_literal_and_expansion() {
    assert_eq!(
        parse("file_${name}.txt"),
        vec![
            Fragment::Literal("file_".into()),
            Fragment::Parameter(ParameterExpansion::Complex {
                name: "name".into(),
                operator: None,
                argument: None,
            }),
            Fragment::Literal(".txt".into()),
        ]
    );
}

#[test]
fn lone_dollar_is_literal() {
    assert_eq!(parse("$"), vec![Fragment::Literal("$".into())]);
}

#[test]
fn concatenated_quoting() {
    // he'llo '"wor"ld  → parts: Literal("he"), SingleQuoted("llo "), DoubleQuoted(Literal("wor")), Literal("ld")
    let parts = parse("he'llo '\"wor\"ld");
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0], Fragment::Literal("he".into()));
    assert_eq!(parts[1], Fragment::SingleQuoted("llo ".into()));
    assert_eq!(
        parts[2],
        Fragment::DoubleQuoted(vec![Fragment::Literal("wor".into())])
    );
    assert_eq!(parts[3], Fragment::Literal("ld".into()));
}
