#[path = "bash/arithmetic.rs"]
mod arithmetic;
#[path = "bash/double_bracket.rs"]
mod double_bracket;

use thaum::ast::*;
use thaum::{parse, parse_with, Dialect, ShellOptions};

#[skuld::test]
fn bash_here_string() {
    let opts = ShellOptions {
        here_strings: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("cat <<< hello", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(&cmd.redirects[0].kind, RedirectKind::BashHereString(_)));
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn posix_rejects_here_string() {
    assert!(parse("cat <<< hello").is_err());
}

#[skuld::test]
fn bash_ampersand_redirect() {
    let prog = parse_with("cmd &> /dev/null", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(&cmd.redirects[0].kind, RedirectKind::BashOutputAll(_)));
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn bash_ampersand_append_redirect() {
    let prog = parse_with("cmd &>> log", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(&cmd.redirects[0].kind, RedirectKind::BashAppendAll(_)));
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn posix_ampersand_is_background() {
    let result = parse("cmd &> /dev/null");
    if let Ok(prog) = &result {
        assert!(prog.lines[0][0].mode == ExecutionMode::Background);
    }
}

#[skuld::test]
fn bash_double_brackets() {
    let opts = ShellOptions {
        double_brackets: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options(r#"[[ -f /etc/passwd ]]"#, opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::BashDoubleBracket { expression, .. },
        ..
    } = &stmt.expression
    {
        // -f /etc/passwd → Unary { op: FileIsRegular, arg: /etc/passwd }
        assert!(matches!(
            expression,
            BashTestExpr::Unary {
                op: UnaryTestOp::FileIsRegular,
                ..
            }
        ));
    } else {
        panic!("expected DoubleBracket, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn bash_double_brackets_with_and() {
    let prog = parse_with(r#"[[ -f foo && -d bar ]]"#, Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::BashDoubleBracket { expression, .. },
        ..
    } = &stmt.expression
    {
        // -f foo && -d bar → And { Unary(-f, foo), Unary(-d, bar) }
        if let BashTestExpr::And { left, right } = expression {
            assert!(matches!(
                left.as_ref(),
                BashTestExpr::Unary {
                    op: UnaryTestOp::FileIsRegular,
                    ..
                }
            ));
            assert!(matches!(
                right.as_ref(),
                BashTestExpr::Unary {
                    op: UnaryTestOp::FileIsDirectory,
                    ..
                }
            ));
        } else {
            panic!("expected And, got {expression:?}");
        }
    } else {
        panic!("expected DoubleBracket");
    }
}

#[skuld::test]
fn bash_double_brackets_requires_space() {
    // [[-f is NOT [[ -f — bash requires whitespace after [[.
    // Without space, [[-f is a literal command name.
    let prog = parse_with("[[-f foo ]]", Dialect::Bash).unwrap();
    assert!(matches!(&prog.lines[0][0].expression, Expression::Command(_)));
}

#[skuld::test]
fn posix_rejects_double_brackets() {
    let prog = parse("[[ -f foo ]]").unwrap();
    let stmt = &prog.lines[0][0];
    assert!(matches!(stmt.expression, Expression::Command(_)));
}

#[skuld::test]
fn bash_arithmetic_command() {
    let opts = ShellOptions {
        arithmetic_command: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("(( x + 1 ))", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::BashArithmeticCommand { expression, .. },
        ..
    } = &stmt.expression
    {
        assert_eq!(
            *expression,
            ArithExpr::Binary {
                left: Box::new(ArithExpr::Variable("x".to_string())),
                op: ArithBinaryOp::Add,
                right: Box::new(ArithExpr::Number(1)),
            }
        );
    } else {
        panic!("expected ArithmeticCommand, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn posix_double_paren_is_subshell() {
    let result = parse("(( x + 1 ))");
    if let Ok(prog) = &result {
        assert!(!matches!(
            &prog.lines[0][0].expression,
            Expression::Compound {
                body: CompoundCommand::BashArithmeticCommand { .. },
                ..
            }
        ));
    }
}

#[skuld::test]
fn bash_nested_subshell_not_arithmetic() {
    // `( (echo hello) )` — space-separated parens are nested subshells, not `((`.
    // The lexer produces two separate LParen tokens; the parser must not
    // collapse them into an arithmetic command.
    // Source: /usr/share/doc/socat/examples/test.sh
    let input = "( (echo hello) )";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[skuld::test]
fn bash_nested_subshell_in_pipeline() {
    // Source: /usr/share/doc/socat/examples/test.sh
    let input = "( (echo a; echo b) | cat ) &";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[skuld::test]
fn bash_function_keyword() {
    let prog = parse_with("function greet { echo hello; }", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::FunctionDef(f) = &stmt.expression {
        assert_eq!(f.name, "greet");
    } else {
        panic!("expected FunctionDef, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn bash_function_keyword_with_parens() {
    let prog = parse_with("function greet() { echo hello; }", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::FunctionDef(f) = &stmt.expression {
        assert_eq!(f.name, "greet");
    } else {
        panic!("expected FunctionDef");
    }
}

#[skuld::test]
fn posix_rejects_function_keyword() {
    let result = parse("function greet { echo hello; }");
    if let Ok(prog) = &result {
        assert!(!matches!(&prog.lines[0][0].expression, Expression::FunctionDef(_)));
    }
}

#[skuld::test]
fn bash_process_substitution_input() {
    let opts = ShellOptions {
        process_substitution: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("diff <(sort a) <(sort b)", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.arguments.len(), 3);
        assert!(matches!(
            &cmd.arguments[1],
            Argument::Atom(Atom::BashProcessSubstitution {
                direction: ProcessDirection::In,
                ..
            })
        ));
        assert!(matches!(
            &cmd.arguments[2],
            Argument::Atom(Atom::BashProcessSubstitution {
                direction: ProcessDirection::In,
                ..
            })
        ));
    } else {
        panic!("expected Command, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn bash_process_substitution_output() {
    let opts = ShellOptions {
        process_substitution: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("tee >(grep err > log)", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.arguments.len(), 2);
        assert!(matches!(
            &cmd.arguments[1],
            Argument::Atom(Atom::BashProcessSubstitution {
                direction: ProcessDirection::Out,
                ..
            })
        ));
    } else {
        panic!("expected Command, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn process_substitution_requires_whitespace() {
    // In bash, `<(` is process substitution only when preceded by whitespace.
    // `foo<(sort a)` has no space before `<`, so `<` is treated as a redirect
    // operator. Since `(sort a)` is not a valid filename (starts with `(`),
    // this is a parse error — matching bash behavior.
    let opts = ShellOptions {
        process_substitution: true,
        ..Default::default()
    };
    let result = thaum::parser::parse_with_options("echo foo<(sort a)", opts.clone());
    assert!(result.is_err());

    // With a space, `< <(sort a)` IS valid: redirect from process substitution
    let prog = thaum::parser::parse_with_options("echo foo < <(sort a)", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Input(_)));
    } else {
        panic!("expected Command, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn posix_rejects_process_substitution() {
    let result = parse("diff <(sort a)");
    if let Ok(prog) = &result {
        if let Expression::Command(cmd) = &prog.lines[0][0].expression {
            assert!(!cmd
                .arguments
                .iter()
                .any(|arg| matches!(arg, Argument::Atom(Atom::BashProcessSubstitution { .. }))));
        }
    }
}

#[skuld::test]
fn bash_extended_case_fall_through() {
    let prog = parse_with("case x in\na) echo a;;&\nb) echo b;;\nesac", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::CaseClause { arms, .. },
        ..
    } = &stmt.expression
    {
        assert_eq!(arms.len(), 2);
        assert_eq!(arms[0].terminator, Some(CaseTerminator::BashFallThrough));
        assert_eq!(arms[1].terminator, Some(CaseTerminator::Break));
    } else {
        panic!("expected CaseClause");
    }
}

#[skuld::test]
fn bash_extended_case_continue() {
    let prog = parse_with("case x in\na) echo a;&\nb) echo b;;\nesac", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::CaseClause { arms, .. },
        ..
    } = &stmt.expression
    {
        assert_eq!(arms.len(), 2);
        assert_eq!(arms[0].terminator, Some(CaseTerminator::BashContinue));
    } else {
        panic!("expected CaseClause");
    }
}

#[skuld::test]
fn bash_select_loop() {
    let opts = ShellOptions {
        select: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("select opt in a b c; do echo $opt; done", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::BashSelectClause {
            variable, words, body, ..
        },
        ..
    } = &stmt.expression
    {
        assert_eq!(variable, "opt");
        assert_eq!(words.as_ref().unwrap().len(), 3);
        assert!(!body.is_empty());
    } else {
        panic!("expected BashSelectClause, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn bash_select_no_in() {
    let opts = ShellOptions {
        select: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("select opt\ndo\necho $opt\ndone", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::BashSelectClause { variable, words, .. },
        ..
    } = &stmt.expression
    {
        assert_eq!(variable, "opt");
        assert!(words.is_none());
    } else {
        panic!("expected BashSelectClause");
    }
}

#[skuld::test]
fn posix_rejects_select() {
    let result = parse("select opt in a b c; do echo $opt; done");
    if let Ok(prog) = &result {
        assert!(matches!(&prog.lines[0][0].expression, Expression::Command(_)));
    }
}

#[skuld::test]
fn bash_coproc_simple() {
    let opts = ShellOptions {
        coproc: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("coproc cat", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::BashCoproc { name, body, .. },
        ..
    } = &stmt.expression
    {
        assert!(name.is_none());
        assert!(matches!(**body, Expression::Command(_)));
    } else {
        panic!("expected BashCoproc, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn bash_coproc_named() {
    let opts = ShellOptions {
        coproc: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("coproc mycoproc { cat; }", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Compound {
        body: CompoundCommand::BashCoproc { name, body, .. },
        ..
    } = &stmt.expression
    {
        assert_eq!(name.as_deref(), Some("mycoproc"));
        assert!(matches!(
            &**body,
            Expression::Compound {
                body: CompoundCommand::BraceGroup { .. },
                ..
            }
        ));
    } else {
        panic!("expected BashCoproc, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn posix_rejects_coproc() {
    let result = parse("coproc cat");
    if let Ok(prog) = &result {
        assert!(matches!(&prog.lines[0][0].expression, Expression::Command(_)));
    }
}

#[skuld::test]
fn bash_array_assignment() {
    let opts = ShellOptions {
        arrays: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("arr=(one two three)", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.assignments.len(), 1);
        assert_eq!(cmd.assignments[0].name, "arr");
        if let AssignmentValue::BashArray(elements) = &cmd.assignments[0].value {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("expected BashArray, got {:?}", cmd.assignments[0].value);
        }
    } else {
        panic!("expected Command, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn bash_array_assignment_empty() {
    let opts = ShellOptions {
        arrays: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("arr=()", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.assignments.len(), 1);
        if let AssignmentValue::BashArray(elements) = &cmd.assignments[0].value {
            assert!(elements.is_empty());
        } else {
            panic!("expected BashArray");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn bash_array_assignment_with_command() {
    let opts = ShellOptions {
        arrays: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("arr=(a b) echo hello", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.assignments.len(), 1);
        assert_eq!(cmd.arguments.len(), 2);
    } else {
        panic!("expected Command");
    }
}

// |& pipe stderr ------------------------------------------------------------------------------------------------------

#[skuld::test]
fn bash_pipe_stderr() {
    // cmd1 |& cmd2 — pipe both stdout and stderr
    let opts = ShellOptions {
        pipe_stderr: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("cmd1 |& cmd2", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Pipe { left, right, stderr } = &stmt.expression {
        assert!(stderr);
        assert!(matches!(left.as_ref(), Expression::Command(_)));
        assert!(matches!(right.as_ref(), Expression::Command(_)));
    } else {
        panic!("expected Pipe, got {:?}", stmt.expression);
    }
}

#[skuld::test]
fn bash_pipe_stderr_in_chain() {
    // a |& b | c — first pipe has stderr, second doesn't
    let opts = ShellOptions {
        pipe_stderr: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("a |& b | c", opts).unwrap();
    let stmt = &prog.lines[0][0];
    // Left-associative: Pipe(Pipe(a, b, stderr=true), c, stderr=false)
    if let Expression::Pipe {
        left,
        stderr: outer_stderr,
        ..
    } = &stmt.expression
    {
        assert!(!outer_stderr); // outer pipe is regular
        if let Expression::Pipe {
            stderr: inner_stderr, ..
        } = left.as_ref()
        {
            assert!(inner_stderr); // inner pipe has stderr
        } else {
            panic!("expected inner Pipe");
        }
    } else {
        panic!("expected Pipe");
    }
}

#[skuld::test]
fn posix_pipe_ampersand_is_background() {
    // In POSIX, `cmd1 |& cmd2` is `cmd1 |` (pipe) then `& cmd2` (background).
    // `cmd1 |` alone is an error (missing right side of pipe).
    // But actually: `|` then `&` — the `|` expects a command on the right,
    // and `&` is not a valid command start.
    let result = parse("cmd1 |& cmd2");
    // This should be an error or parse differently from Bash mode
    if let Ok(prog) = &result {
        // If it parses, it should NOT be a Pipe with stderr
        assert!(!matches!(
            &prog.lines[0][0].expression,
            Expression::Pipe { stderr: true, .. }
        ));
    }
}

// $'...' ANSI-C quoting -----------------------------------------------------------------------------------------------

#[skuld::test]
fn bash_ansi_c_quoting() {
    let opts = ShellOptions {
        ansi_c_quoting: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options(r"echo $'\n\t'", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.arguments.len(), 2);
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(matches!(&w.parts[0], Fragment::BashAnsiCQuoted(_)));
            if let Fragment::BashAnsiCQuoted(s) = &w.parts[0] {
                assert_eq!(s, r"\n\t");
            }
        } else {
            panic!("expected Word argument");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn bash_ansi_c_quoting_concatenated() {
    // prefix$'\n'suffix — three fragments
    let opts = ShellOptions {
        ansi_c_quoting: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options(r"echo prefix$'\n'suffix", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert_eq!(w.parts.len(), 3);
            assert!(matches!(&w.parts[0], Fragment::Literal(s) if s == "prefix"));
            assert!(matches!(&w.parts[1], Fragment::BashAnsiCQuoted(_)));
            assert!(matches!(&w.parts[2], Fragment::Literal(s) if s == "suffix"));
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn posix_dollar_single_quote_is_dollar_plus_string() {
    // In POSIX, $'...' is just $ followed by a single-quoted string
    let prog = parse(r"echo $'hello'").unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            // Should NOT be BashAnsiCQuoted
            assert!(!w.parts.iter().any(|p| matches!(p, Fragment::BashAnsiCQuoted(_))));
        }
    }
}

// $"..." locale translation -------------------------------------------------------------------------------------------

#[skuld::test]
fn bash_locale_quoted() {
    let opts = ShellOptions {
        locale_translation: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options(r#"echo $"hello $USER""#, opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(matches!(&w.parts[0], Fragment::BashLocaleQuoted { .. }));
            if let Fragment::BashLocaleQuoted { raw, parts } = &w.parts[0] {
                assert_eq!(raw, "hello $USER");
                // Should contain at least a Literal and a Parameter
                assert!(parts.iter().any(|p| matches!(p, Fragment::Literal(_))));
                assert!(parts.iter().any(|p| matches!(p, Fragment::Parameter(_))));
            }
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn posix_dollar_double_quote_is_dollar_plus_string() {
    // In POSIX, $"..." is just $ followed by a double-quoted string
    let prog = parse(r#"echo $"hello""#).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(!w.parts.iter().any(|p| matches!(p, Fragment::BashLocaleQuoted { .. })));
        }
    }
}

// extglob -------------------------------------------------------------------------------------------------------------

#[skuld::test]
fn bash_extglob_zero_or_more() {
    let opts = ShellOptions {
        extglob: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("echo *(*.txt)", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(matches!(
                &w.parts[0],
                Fragment::BashExtGlob {
                    kind: ExtGlobKind::ZeroOrMore,
                    ..
                }
            ));
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn bash_extglob_not() {
    let opts = ShellOptions {
        extglob: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("echo !(*.bak)", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(matches!(
                &w.parts[0],
                Fragment::BashExtGlob {
                    kind: ExtGlobKind::Not,
                    ..
                }
            ));
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn bash_extglob_in_word() {
    // file.@(txt|md) — extglob after a literal prefix
    let opts = ShellOptions {
        extglob: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("echo file.@(txt|md)", opts).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(matches!(&w.parts[0], Fragment::Literal(s) if s == "file."));
            assert!(matches!(
                &w.parts[1],
                Fragment::BashExtGlob {
                    kind: ExtGlobKind::ExactlyOne,
                    ..
                }
            ));
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn posix_rejects_extglob() {
    // TODO: In POSIX, *(foo) is glob * followed by subshell (foo). Our parser
    // currently errors because * becomes a word and ( starts a subshell which
    // may not be valid in this position. The important thing is no BashExtGlob.
    // In POSIX, *(foo) is glob * followed by subshell (foo)
    // Since * is a glob and ( starts a subshell, this should NOT produce BashExtGlob
    let result = parse("echo *(*.txt)");
    if let Ok(prog) = &result {
        if let Expression::Command(cmd) = &prog.lines[0][0].expression {
            for arg in &cmd.arguments {
                if let Argument::Word(w) = arg {
                    assert!(!w.parts.iter().any(|p| matches!(p, Fragment::BashExtGlob { .. })));
                }
            }
        }
    }
}

// brace expansion -----------------------------------------------------------------------------------------------------

#[skuld::test]
fn bash_brace_expansion_list() {
    let prog = parse_with("echo {a,b,c}", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(matches!(
                &w.parts[0],
                Fragment::BashBraceExpansion(BraceExpansionKind::List(_))
            ));
            if let Fragment::BashBraceExpansion(BraceExpansionKind::List(items)) = &w.parts[0] {
                assert_eq!(items.len(), 3);
            }
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn bash_brace_expansion_sequence() {
    let prog = parse_with("echo {1..5}", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            if let Fragment::BashBraceExpansion(BraceExpansionKind::Sequence { start, end, step }) = &w.parts[0] {
                assert_eq!(start, "1");
                assert_eq!(end, "5");
                assert!(step.is_none());
            } else {
                panic!("expected Sequence, got {:?}", w.parts[0]);
            }
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn bash_brace_expansion_step() {
    let prog = parse_with("echo {0..10..2}", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            if let Fragment::BashBraceExpansion(BraceExpansionKind::Sequence { start, end, step }) = &w.parts[0] {
                assert_eq!(start, "0");
                assert_eq!(end, "10");
                assert_eq!(step.as_deref(), Some("2"));
            } else {
                panic!("expected Sequence, got {:?}", w.parts[0]);
            }
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn bash_brace_expansion_in_word() {
    let prog = parse_with("echo file{1,2,3}.txt", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(matches!(&w.parts[0], Fragment::Literal(s) if s == "file"));
            assert!(matches!(
                &w.parts[1],
                Fragment::BashBraceExpansion(BraceExpansionKind::List(_))
            ));
            assert!(matches!(&w.parts[2], Fragment::Literal(s) if s == ".txt"));
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[skuld::test]
fn posix_brace_is_literal() {
    let prog = parse("echo {a,b,c}").unwrap();
    let stmt = &prog.lines[0][0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(!w.parts.iter().any(|p| matches!(p, Fragment::BashBraceExpansion(_))));
        }
    }
}
