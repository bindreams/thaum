mod common;

use common::*;
use thaum::ast::*;
use thaum::parse;

fn main() {
    testutil::run_all();
}

testutil::default_labels!(lex, parse);

#[testutil::test]
fn if_with_test_command() {
    let compound = first_compound(r#"if [ "$x" = "yes" ]; then echo matched; fi"#);
    if let CompoundCommand::IfClause {
        condition, then_body, ..
    } = &compound
    {
        assert!(!condition.is_empty());
        assert!(!then_body.is_empty());
    } else {
        panic!("expected if clause");
    }
}

#[testutil::test]
fn nested_if() {
    let input = r#"if true; then
    if false; then
        echo inner
    fi
fi"#;
    let compound = first_compound(input);
    if let CompoundCommand::IfClause { then_body, .. } = &compound {
        assert!(matches!(
            then_body[0][0].expression,
            Expression::Compound {
                body: CompoundCommand::IfClause { .. },
                ..
            }
        ));
    } else {
        panic!("expected if clause");
    }
}

#[testutil::test]
fn if_elif_else() {
    let compound = first_compound(
        r#"if [ -f /etc/config ]; then
    . /etc/config
elif [ -f ~/.config ]; then
    . ~/.config
else
    echo "No config found"
fi"#,
    );
    if let CompoundCommand::IfClause { elifs, else_body, .. } = &compound {
        assert_eq!(elifs.len(), 1);
        assert!(else_body.is_some());
    } else {
        panic!("expected if clause");
    }
}

#[testutil::test]
fn if_with_newlines() {
    assert!(matches!(
        first_compound("if\ntrue\nthen\necho yes\nfi"),
        CompoundCommand::IfClause { .. }
    ));
}

#[testutil::test]
fn while_read_loop() {
    assert!(matches!(
        first_compound("while read line; do\n    echo \"$line\"\ndone"),
        CompoundCommand::WhileClause { .. }
    ));
}

#[testutil::test]
fn for_loop_with_glob() {
    let compound = first_compound("for f in *.txt; do echo $f; done");
    if let CompoundCommand::ForClause {
        variable, words, body, ..
    } = &compound
    {
        assert_eq!(variable, "f");
        let word_list = words.as_ref().unwrap();
        assert_eq!(word_list.len(), 1);
        assert!(word_list[0]
            .parts
            .iter()
            .any(|p| matches!(p, Fragment::Glob(GlobChar::Star))));
        assert!(!body.is_empty());
    } else {
        panic!("expected for clause");
    }
}

#[testutil::test]
fn for_loop_with_newline_instead_of_semicolon() {
    if let CompoundCommand::ForClause { words, .. } = &first_compound("for i in a b c\ndo\necho $i\ndone") {
        assert_eq!(words.as_ref().unwrap().len(), 3);
    } else {
        panic!("expected for clause");
    }
}

#[testutil::test]
fn case_with_multiple_patterns() {
    let input = r#"case "$1" in
    start|begin)
        echo starting
        ;;
    stop|end)
        echo stopping
        ;;
    *)
        echo "unknown: $1"
        ;;
esac"#;
    let compound = first_compound(input);
    if let CompoundCommand::CaseClause { arms, .. } = &compound {
        assert_eq!(arms.len(), 3);
        assert_eq!(arms[0].patterns.len(), 2);
    } else {
        panic!("expected case clause");
    }
}

#[testutil::test]
fn empty_case_arms() {
    let compound = first_compound("case x in\na) ;;\nb) ;;\nesac");
    if let CompoundCommand::CaseClause { arms, .. } = &compound {
        assert_eq!(arms.len(), 2);
        assert!(arms[0].body.is_empty());
    } else {
        panic!("expected case clause");
    }
}

#[testutil::test]
fn case_pattern_backslash_newline_with_indent() {
    // Case pattern continued across lines with `\<newline>` and indentation.
    // Source: /usr/bin/gzexe, /usr/bin/nroff, /usr/bin/xzgrep, /usr/bin/zgrep,
    //         /usr/lib/git-core/git-web--browse, /usr/bin/ldd
    let input = "case $x in\n  aaa | bbb | \\\n  ccc) echo match;;\nesac";
    assert!(
        parse(input).is_ok(),
        "case pattern with backslash-newline and indentation"
    );
}

#[testutil::test]
fn brace_group_with_redirect() {
    let e = first_expr("{ echo hello; echo world; } > output.txt");
    if let Expression::Compound {
        body: CompoundCommand::BraceGroup { body, .. },
        redirects,
    } = &e
    {
        assert_eq!(body.iter().flatten().count(), 2);
        assert_eq!(redirects.len(), 1);
    } else {
        panic!("expected brace group with redirect");
    }
}

#[testutil::test]
fn until_loop() {
    assert!(matches!(
        first_compound("until false; do echo waiting; done"),
        CompoundCommand::UntilClause { .. }
    ));
}

#[testutil::test]
fn for_without_in_clause() {
    // `for var; do ...; done` iterates over $@
    let compound = first_compound("for arg; do echo $arg; done");
    if let CompoundCommand::ForClause { variable, words, .. } = &compound {
        assert_eq!(variable, "arg");
        assert!(words.is_none());
    } else {
        panic!("expected for clause");
    }
}

#[testutil::test]
fn deeply_nested_compound() {
    let input = r#"if true; then
    while true; do
        for i in 1 2 3; do
            if false; then
                echo deep
            fi
        done
    done
fi"#;
    let prog = parse_ok(input);
    assert!(!prog.lines.is_empty());
}

#[testutil::test]
fn bash_empty_then_fi() {
    let compound = first_compound_bash("if true; then\nfi");
    if let CompoundCommand::IfClause { then_body, .. } = &compound {
        assert!(then_body.is_empty());
    } else {
        panic!("expected if clause");
    }
}

#[testutil::test]
fn bash_empty_do_done() {
    let compound = first_compound_bash("while false; do\ndone");
    if let CompoundCommand::WhileClause { body, .. } = &compound {
        assert!(body.is_empty());
    } else {
        panic!("expected while clause");
    }
}

#[testutil::test]
fn bash_empty_for_body() {
    let compound = first_compound_bash("for i in a b; do\ndone");
    if let CompoundCommand::ForClause { body, .. } = &compound {
        assert!(body.is_empty());
    } else {
        panic!("expected for clause");
    }
}

#[testutil::test]
fn posix_rejects_empty_then_fi() {
    assert!(parse("if true; then\nfi").is_err());
}

#[testutil::test]
fn posix_rejects_empty_do_done() {
    assert!(parse("while false; do\ndone").is_err());
}
