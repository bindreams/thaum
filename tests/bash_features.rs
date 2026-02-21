use thaum::ast::*;
use thaum::{parse, parse_with, Dialect, ParseOptions};

#[test]
fn bash_here_string() {
    let mut opts = ParseOptions::default();
    opts.here_strings = true;
    let prog = thaum::parser::parse_with_options("cat <<< hello", opts).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(
            &cmd.redirects[0].kind,
            RedirectKind::BashHereString(_)
        ));
    } else {
        panic!("expected Command");
    }
}

#[test]
fn posix_rejects_here_string() {
    assert!(parse("cat <<< hello").is_err());
}

#[test]
fn bash_ampersand_redirect() {
    let prog = parse_with("cmd &> /dev/null", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(
            &cmd.redirects[0].kind,
            RedirectKind::BashOutputAll(_)
        ));
    } else {
        panic!("expected Command");
    }
}

#[test]
fn bash_ampersand_append_redirect() {
    let prog = parse_with("cmd &>> log", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(
            &cmd.redirects[0].kind,
            RedirectKind::BashAppendAll(_)
        ));
    } else {
        panic!("expected Command");
    }
}

#[test]
fn posix_ampersand_is_background() {
    let result = parse("cmd &> /dev/null");
    if let Ok(prog) = &result {
        assert!(prog.statements[0].mode == ExecutionMode::Background);
    }
}

#[test]
fn bash_double_brackets() {
    let mut opts = ParseOptions::default();
    opts.double_brackets = true;
    let prog = thaum::parser::parse_with_options(r#"[[ -f /etc/passwd ]]"#, opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_double_brackets_with_and() {
    let prog = parse_with(r#"[[ -f foo && -d bar ]]"#, Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
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
            panic!("expected And, got {:?}", expression);
        }
    } else {
        panic!("expected DoubleBracket");
    }
}

#[test]
fn bash_double_brackets_no_space() {
    let prog = parse_with("[[-f foo ]]", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Compound {
        body: CompoundCommand::BashDoubleBracket { expression, .. },
        ..
    } = &stmt.expression
    {
        // [[-f is lexed as [[ then -f, so: Unary { op: FileIsRegular, arg: foo }
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

#[test]
fn posix_rejects_double_brackets() {
    let prog = parse("[[ -f foo ]]").unwrap();
    let stmt = &prog.statements[0];
    assert!(matches!(stmt.expression, Expression::Command(_)));
}

#[test]
fn bash_arithmetic_command() {
    let mut opts = ParseOptions::default();
    opts.arithmetic_command = true;
    let prog = thaum::parser::parse_with_options("(( x + 1 ))", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn posix_double_paren_is_subshell() {
    let result = parse("(( x + 1 ))");
    if let Ok(prog) = &result {
        assert!(!matches!(
            &prog.statements[0].expression,
            Expression::Compound {
                body: CompoundCommand::BashArithmeticCommand { .. },
                ..
            }
        ));
    }
}

#[test]
fn bash_nested_subshell_not_arithmetic() {
    // `( (echo hello) )` — space-separated parens are nested subshells, not `((`.
    // The lexer produces two separate LParen tokens; the parser must not
    // collapse them into an arithmetic command.
    // Source: /usr/share/doc/socat/examples/test.sh
    let input = "( (echo hello) )";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[test]
fn bash_nested_subshell_in_pipeline() {
    // Source: /usr/share/doc/socat/examples/test.sh
    let input = "( (echo a; echo b) | cat ) &";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[test]
fn bash_function_keyword() {
    let prog = parse_with("function greet { echo hello; }", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::FunctionDef(f) = &stmt.expression {
        assert_eq!(f.name, "greet");
    } else {
        panic!("expected FunctionDef, got {:?}", stmt.expression);
    }
}

#[test]
fn bash_function_keyword_with_parens() {
    let prog = parse_with("function greet() { echo hello; }", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::FunctionDef(f) = &stmt.expression {
        assert_eq!(f.name, "greet");
    } else {
        panic!("expected FunctionDef");
    }
}

#[test]
fn posix_rejects_function_keyword() {
    let result = parse("function greet { echo hello; }");
    if let Ok(prog) = &result {
        assert!(!matches!(
            &prog.statements[0].expression,
            Expression::FunctionDef(_)
        ));
    }
}

#[test]
fn bash_process_substitution_input() {
    let mut opts = ParseOptions::default();
    opts.process_substitution = true;
    let prog = thaum::parser::parse_with_options("diff <(sort a) <(sort b)", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_process_substitution_output() {
    let mut opts = ParseOptions::default();
    opts.process_substitution = true;
    let prog = thaum::parser::parse_with_options("tee >(grep err > log)", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn process_substitution_requires_whitespace() {
    // In bash, `<(` is process substitution only when preceded by whitespace.
    // `foo<(sort a)` has no space before `<`, so `<` is treated as a redirect
    // operator. Since `(sort a)` is not a valid filename (starts with `(`),
    // this is a parse error — matching bash behavior.
    let mut opts = ParseOptions::default();
    opts.process_substitution = true;
    let result = thaum::parser::parse_with_options("echo foo<(sort a)", opts.clone());
    assert!(result.is_err());

    // With a space, `< <(sort a)` IS valid: redirect from process substitution
    let prog = thaum::parser::parse_with_options("echo foo < <(sort a)", opts).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Input(_)));
    } else {
        panic!("expected Command, got {:?}", stmt.expression);
    }
}

#[test]
fn posix_rejects_process_substitution() {
    let result = parse("diff <(sort a)");
    if let Ok(prog) = &result {
        if let Expression::Command(cmd) = &prog.statements[0].expression {
            assert!(!cmd
                .arguments
                .iter()
                .any(|arg| matches!(arg, Argument::Atom(Atom::BashProcessSubstitution { .. }))));
        }
    }
}

#[test]
fn bash_extended_case_fall_through() {
    let prog = parse_with("case x in\na) echo a;;&\nb) echo b;;\nesac", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_extended_case_continue() {
    let prog = parse_with("case x in\na) echo a;&\nb) echo b;;\nesac", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_select_loop() {
    let mut opts = ParseOptions::default();
    opts.select = true;
    let prog =
        thaum::parser::parse_with_options("select opt in a b c; do echo $opt; done", opts)
            .unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Compound {
        body:
            CompoundCommand::BashSelectClause {
                variable,
                words,
                body,
                ..
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

#[test]
fn bash_select_no_in() {
    let mut opts = ParseOptions::default();
    opts.select = true;
    let prog =
        thaum::parser::parse_with_options("select opt\ndo\necho $opt\ndone", opts).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Compound {
        body: CompoundCommand::BashSelectClause {
            variable, words, ..
        },
        ..
    } = &stmt.expression
    {
        assert_eq!(variable, "opt");
        assert!(words.is_none());
    } else {
        panic!("expected BashSelectClause");
    }
}

#[test]
fn posix_rejects_select() {
    let result = parse("select opt in a b c; do echo $opt; done");
    if let Ok(prog) = &result {
        assert!(matches!(
            &prog.statements[0].expression,
            Expression::Command(_)
        ));
    }
}

#[test]
fn bash_coproc_simple() {
    let mut opts = ParseOptions::default();
    opts.coproc = true;
    let prog = thaum::parser::parse_with_options("coproc cat", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_coproc_named() {
    let mut opts = ParseOptions::default();
    opts.coproc = true;
    let prog = thaum::parser::parse_with_options("coproc mycoproc { cat; }", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn posix_rejects_coproc() {
    let result = parse("coproc cat");
    if let Ok(prog) = &result {
        assert!(matches!(
            &prog.statements[0].expression,
            Expression::Command(_)
        ));
    }
}

#[test]
fn bash_array_assignment() {
    let mut opts = ParseOptions::default();
    opts.arrays = true;
    let prog = thaum::parser::parse_with_options("arr=(one two three)", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_array_assignment_empty() {
    let mut opts = ParseOptions::default();
    opts.arrays = true;
    let prog = thaum::parser::parse_with_options("arr=()", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_array_assignment_with_command() {
    let mut opts = ParseOptions::default();
    opts.arrays = true;
    let prog = thaum::parser::parse_with_options("arr=(a b) echo hello", opts).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.assignments.len(), 1);
        assert_eq!(cmd.arguments.len(), 2);
    } else {
        panic!("expected Command");
    }
}

// --- |& pipe stderr ---

#[test]
fn bash_pipe_stderr() {
    // cmd1 |& cmd2 — pipe both stdout and stderr
    let mut opts = ParseOptions::default();
    opts.pipe_stderr = true;
    let prog = thaum::parser::parse_with_options("cmd1 |& cmd2", opts).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Pipe {
        left,
        right,
        stderr,
    } = &stmt.expression
    {
        assert!(stderr);
        assert!(matches!(left.as_ref(), Expression::Command(_)));
        assert!(matches!(right.as_ref(), Expression::Command(_)));
    } else {
        panic!("expected Pipe, got {:?}", stmt.expression);
    }
}

#[test]
fn bash_pipe_stderr_in_chain() {
    // a |& b | c — first pipe has stderr, second doesn't
    let mut opts = ParseOptions::default();
    opts.pipe_stderr = true;
    let prog = thaum::parser::parse_with_options("a |& b | c", opts).unwrap();
    let stmt = &prog.statements[0];
    // Left-associative: Pipe(Pipe(a, b, stderr=true), c, stderr=false)
    if let Expression::Pipe {
        left,
        stderr: outer_stderr,
        ..
    } = &stmt.expression
    {
        assert!(!outer_stderr); // outer pipe is regular
        if let Expression::Pipe {
            stderr: inner_stderr,
            ..
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

#[test]
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
            &prog.statements[0].expression,
            Expression::Pipe { stderr: true, .. }
        ));
    }
}

// --- $'...' ANSI-C quoting ---

#[test]
fn bash_ansi_c_quoting() {
    let mut opts = ParseOptions::default();
    opts.ansi_c_quoting = true;
    let prog = thaum::parser::parse_with_options(r"echo $'\n\t'", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_ansi_c_quoting_concatenated() {
    // prefix$'\n'suffix — three fragments
    let mut opts = ParseOptions::default();
    opts.ansi_c_quoting = true;
    let prog = thaum::parser::parse_with_options(r"echo prefix$'\n'suffix", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn posix_dollar_single_quote_is_dollar_plus_string() {
    // In POSIX, $'...' is just $ followed by a single-quoted string
    let prog = parse(r"echo $'hello'").unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            // Should NOT be BashAnsiCQuoted
            assert!(!w
                .parts
                .iter()
                .any(|p| matches!(p, Fragment::BashAnsiCQuoted(_))));
        }
    }
}

// --- $"..." locale translation ---

#[test]
fn bash_locale_quoted() {
    let mut opts = ParseOptions::default();
    opts.locale_translation = true;
    let prog = thaum::parser::parse_with_options(r#"echo $"hello $USER""#, opts).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(matches!(&w.parts[0], Fragment::BashLocaleQuoted(_)));
            if let Fragment::BashLocaleQuoted(inner) = &w.parts[0] {
                // Should contain at least a Literal and a Parameter
                assert!(inner.iter().any(|p| matches!(p, Fragment::Literal(_))));
                assert!(inner.iter().any(|p| matches!(p, Fragment::Parameter(_))));
            }
        } else {
            panic!("expected Word");
        }
    } else {
        panic!("expected Command");
    }
}

#[test]
fn posix_dollar_double_quote_is_dollar_plus_string() {
    // In POSIX, $"..." is just $ followed by a double-quoted string
    let prog = parse(r#"echo $"hello""#).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(!w
                .parts
                .iter()
                .any(|p| matches!(p, Fragment::BashLocaleQuoted(_))));
        }
    }
}

// --- extglob ---

#[test]
fn bash_extglob_zero_or_more() {
    let mut opts = ParseOptions::default();
    opts.extglob = true;
    let prog = thaum::parser::parse_with_options("echo *(*.txt)", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_extglob_not() {
    let mut opts = ParseOptions::default();
    opts.extglob = true;
    let prog = thaum::parser::parse_with_options("echo !(*.bak)", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_extglob_in_word() {
    // file.@(txt|md) — extglob after a literal prefix
    let mut opts = ParseOptions::default();
    opts.extglob = true;
    let prog = thaum::parser::parse_with_options("echo file.@(txt|md)", opts).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn posix_rejects_extglob() {
    // TODO: In POSIX, *(foo) is glob * followed by subshell (foo). Our parser
    // currently errors because * becomes a word and ( starts a subshell which
    // may not be valid in this position. The important thing is no BashExtGlob.
    // In POSIX, *(foo) is glob * followed by subshell (foo)
    // Since * is a glob and ( starts a subshell, this should NOT produce BashExtGlob
    let result = parse("echo *(*.txt)");
    if let Ok(prog) = &result {
        if let Expression::Command(cmd) = &prog.statements[0].expression {
            for arg in &cmd.arguments {
                if let Argument::Word(w) = arg {
                    assert!(!w
                        .parts
                        .iter()
                        .any(|p| matches!(p, Fragment::BashExtGlob { .. })));
                }
            }
        }
    }
}

// --- brace expansion ---

#[test]
fn bash_brace_expansion_list() {
    let prog = parse_with("echo {a,b,c}", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn bash_brace_expansion_sequence() {
    let prog = parse_with("echo {1..5}", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            if let Fragment::BashBraceExpansion(BraceExpansionKind::Sequence { start, end, step }) =
                &w.parts[0]
            {
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

#[test]
fn bash_brace_expansion_step() {
    let prog = parse_with("echo {0..10..2}", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            if let Fragment::BashBraceExpansion(BraceExpansionKind::Sequence { start, end, step }) =
                &w.parts[0]
            {
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

#[test]
fn bash_brace_expansion_in_word() {
    let prog = parse_with("echo file{1,2,3}.txt", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
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

#[test]
fn posix_brace_is_literal() {
    let prog = parse("echo {a,b,c}").unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert!(!w
                .parts
                .iter()
                .any(|p| matches!(p, Fragment::BashBraceExpansion(_))));
        }
    }
}

// --- [[ ]] test expression parsing ---

/// Helper: parse input in Bash mode and extract the BashTestExpr.
fn parse_test_expr(input: &str) -> BashTestExpr {
    let prog = parse_with(input, Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Compound {
        body: CompoundCommand::BashDoubleBracket { expression, .. },
        ..
    } = &stmt.expression
    {
        expression.clone()
    } else {
        panic!("expected BashDoubleBracket, got {:?}", stmt.expression);
    }
}

#[test]
fn test_expr_unary_file_exists() {
    let expr = parse_test_expr("[[ -e /tmp/foo ]]");
    assert!(matches!(
        expr,
        BashTestExpr::Unary {
            op: UnaryTestOp::FileExists,
            ..
        }
    ));
}

#[test]
fn test_expr_unary_string_empty() {
    let expr = parse_test_expr("[[ -z $var ]]");
    assert!(matches!(
        expr,
        BashTestExpr::Unary {
            op: UnaryTestOp::StringIsEmpty,
            ..
        }
    ));
}

#[test]
fn test_expr_unary_string_nonempty() {
    let expr = parse_test_expr("[[ -n hello ]]");
    assert!(matches!(
        expr,
        BashTestExpr::Unary {
            op: UnaryTestOp::StringIsNonEmpty,
            ..
        }
    ));
}

#[test]
fn test_expr_binary_string_equals() {
    let expr = parse_test_expr(r#"[[ $a == hello ]]"#);
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringEquals);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_binary_string_not_equals() {
    let expr = parse_test_expr("[[ $a != $b ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringNotEquals);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_binary_int_eq() {
    let expr = parse_test_expr("[[ $x -eq 0 ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::IntEq);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_binary_int_lt() {
    let expr = parse_test_expr("[[ $x -lt 10 ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::IntLt);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_binary_less_than() {
    // < and > are lexed as RedirectFromFile / RedirectToFile tokens
    let expr = parse_test_expr("[[ $a < $b ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringLessThan);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_binary_greater_than() {
    let expr = parse_test_expr("[[ $a > $b ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringGreaterThan);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_binary_regex_match() {
    let expr = parse_test_expr("[[ $str =~ ^[0-9]+$ ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::RegexMatch);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_regex_with_unquoted_parens() {
    // Parentheses in a =~ regex are capturing groups, not shell syntax.
    // Source: /usr/bin/socat-chain.sh
    let input = r#"[[ "$x" =~ ^([^:]*):([^:]*) ]]"#;
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[test]
fn test_expr_regex_with_alternation_in_parens() {
    // Pipe inside regex parens is alternation, not a shell pipe.
    // Source: /usr/lib/snapd/complete.sh
    let input = r#"[[ "${BASH_SOURCE[0]}" =~ ^(/var/lib|/usr/share)/completions/ ]]"#;
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[test]
fn test_expr_regex_with_escaped_parens() {
    // Mixed escaped and unescaped parens in regex.
    // Source: /usr/local/go/.../mkerrors.bash
    let input = r#"[[ $line =~ ^#define\ +([A-Z]+)\ +\(\(([A-Z]+)\)([0-9]+)\) ]]"#;
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[test]
fn test_expr_binary_file_newer() {
    let expr = parse_test_expr("[[ a.txt -nt b.txt ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::FileNewerThan);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_logical_or() {
    let expr = parse_test_expr("[[ -f a || -f b ]]");
    if let BashTestExpr::Or { left, right } = &expr {
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
                op: UnaryTestOp::FileIsRegular,
                ..
            }
        ));
    } else {
        panic!("expected Or, got {:?}", expr);
    }
}

#[test]
fn test_expr_logical_not() {
    let expr = parse_test_expr("[[ ! -f foo ]]");
    if let BashTestExpr::Not(inner) = &expr {
        assert!(matches!(
            inner.as_ref(),
            BashTestExpr::Unary {
                op: UnaryTestOp::FileIsRegular,
                ..
            }
        ));
    } else {
        panic!("expected Not, got {:?}", expr);
    }
}

#[test]
fn test_expr_double_not() {
    // [[ ! ! -f foo ]] → Not(Not(Unary(-f, foo)))
    let expr = parse_test_expr("[[ ! ! -f foo ]]");
    if let BashTestExpr::Not(inner) = &expr {
        assert!(matches!(inner.as_ref(), BashTestExpr::Not(_)));
    } else {
        panic!("expected Not, got {:?}", expr);
    }
}

#[test]
fn test_expr_grouped() {
    let expr = parse_test_expr("[[ ( -f foo ) ]]");
    if let BashTestExpr::Group(inner) = &expr {
        assert!(matches!(
            inner.as_ref(),
            BashTestExpr::Unary {
                op: UnaryTestOp::FileIsRegular,
                ..
            }
        ));
    } else {
        panic!("expected Group, got {:?}", expr);
    }
}

// --- [[ ]] multi-line and edge cases ---

#[test]
fn dbracket_multiline_and() {
    // [[ over multiple lines with &&
    let expr = parse_test_expr("[[ foo == foo\n&& bar == bar\n]]");
    assert!(matches!(expr, BashTestExpr::And { .. }));
}

#[test]
fn dbracket_multiline_or() {
    // [[ over multiple lines with ||
    let expr = parse_test_expr("[[ -f a\n|| -d b\n]]");
    assert!(matches!(expr, BashTestExpr::Or { .. }));
}

// TODO: [[ word]] (no space before ]]) requires context-aware lexing.
// The lexer doesn't recognize ]] inside a word. Bash handles this because
// its parser feeds back to the lexer that it's inside [[ ]].
// #[test]
// fn dbracket_no_space_before_close() {
//     let expr = parse_test_expr("[[ word]]");
//     assert!(matches!(expr, BashTestExpr::Word(_)));
// }

#[test]
fn dbracket_string_gt_no_space() {
    // [[ b>a ]] — string > comparison with no spaces around >
    let expr = parse_test_expr("[[ b>a ]]");
    assert!(matches!(expr, BashTestExpr::Binary { op: BinaryTestOp::StringGreaterThan, .. }));
}

#[test]
fn dbracket_string_lt_no_space() {
    // [[ a<b ]] — string < comparison
    let expr = parse_test_expr("[[ a<b ]]");
    assert!(matches!(expr, BashTestExpr::Binary { op: BinaryTestOp::StringLessThan, .. }));
}

#[test]
fn test_expr_bare_word() {
    // [[ word ]] → implicit -n test
    let expr = parse_test_expr("[[ hello ]]");
    assert!(matches!(expr, BashTestExpr::Word(_)));
}

#[test]
fn test_expr_precedence_and_binds_tighter_than_or() {
    // [[ -f a || -d b && -d c ]] → Or(-f a, And(-d b, -d c))
    let expr = parse_test_expr("[[ -f a || -d b && -d c ]]");
    if let BashTestExpr::Or { left, right } = &expr {
        assert!(matches!(
            left.as_ref(),
            BashTestExpr::Unary {
                op: UnaryTestOp::FileIsRegular,
                ..
            }
        ));
        assert!(matches!(right.as_ref(), BashTestExpr::And { .. }));
    } else {
        panic!("expected Or, got {:?}", expr);
    }
}

#[test]
fn test_expr_not_binds_tighter_than_and() {
    // [[ ! -f a && -d b ]] → And(Not(Unary(-f, a)), Unary(-d, b))
    let expr = parse_test_expr("[[ ! -f a && -d b ]]");
    if let BashTestExpr::And { left, right } = &expr {
        assert!(matches!(left.as_ref(), BashTestExpr::Not(_)));
        assert!(matches!(
            right.as_ref(),
            BashTestExpr::Unary {
                op: UnaryTestOp::FileIsDirectory,
                ..
            }
        ));
    } else {
        panic!("expected And, got {:?}", expr);
    }
}

#[test]
fn test_expr_grouped_or_overrides_precedence() {
    // [[ ( -f a || -d b ) && -e c ]] → And(Group(Or(...)), Unary(-e, c))
    let expr = parse_test_expr("[[ ( -f a || -d b ) && -e c ]]");
    if let BashTestExpr::And { left, right } = &expr {
        assert!(matches!(left.as_ref(), BashTestExpr::Group(_)));
        assert!(matches!(
            right.as_ref(),
            BashTestExpr::Unary {
                op: UnaryTestOp::FileExists,
                ..
            }
        ));
    } else {
        panic!("expected And, got {:?}", expr);
    }
}

#[test]
fn test_expr_binary_eq_single_equals() {
    // [[ $a = pattern ]] — single = is the same as ==
    let expr = parse_test_expr("[[ $a = hello ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringEquals);
    } else {
        panic!("expected Binary, got {:?}", expr);
    }
}

#[test]
fn test_expr_unclosed_double_bracket_is_error() {
    let mut opts = ParseOptions::default();
    opts.double_brackets = true;
    let result = thaum::parser::parse_with_options("[[ -f foo", opts);
    assert!(result.is_err());
}

#[test]
fn test_expr_chained_and() {
    // [[ -f a && -d b && -e c ]] → And(And(-f a, -d b), -e c)
    let expr = parse_test_expr("[[ -f a && -d b && -e c ]]");
    if let BashTestExpr::And { left, right } = &expr {
        assert!(matches!(left.as_ref(), BashTestExpr::And { .. }));
        assert!(matches!(
            right.as_ref(),
            BashTestExpr::Unary {
                op: UnaryTestOp::FileExists,
                ..
            }
        ));
    } else {
        panic!("expected And, got {:?}", expr);
    }
}

#[test]
fn test_expr_chained_or() {
    // [[ -f a || -d b || -e c ]] → Or(Or(-f a, -d b), -e c)
    let expr = parse_test_expr("[[ -f a || -d b || -e c ]]");
    if let BashTestExpr::Or { left, right } = &expr {
        assert!(matches!(left.as_ref(), BashTestExpr::Or { .. }));
        assert!(matches!(
            right.as_ref(),
            BashTestExpr::Unary {
                op: UnaryTestOp::FileExists,
                ..
            }
        ));
    } else {
        panic!("expected Or, got {:?}", expr);
    }
}

#[test]
fn test_expr_all_unary_ops() {
    // Verify all unary operators are recognized
    let cases: Vec<(&str, UnaryTestOp)> = vec![
        ("-e", UnaryTestOp::FileExists),
        ("-f", UnaryTestOp::FileIsRegular),
        ("-d", UnaryTestOp::FileIsDirectory),
        ("-L", UnaryTestOp::FileIsSymlink),
        ("-h", UnaryTestOp::FileIsSymlink),
        ("-b", UnaryTestOp::FileIsBlockDev),
        ("-c", UnaryTestOp::FileIsCharDev),
        ("-p", UnaryTestOp::FileIsPipe),
        ("-S", UnaryTestOp::FileIsSocket),
        ("-s", UnaryTestOp::FileHasSize),
        ("-t", UnaryTestOp::FileDescriptorOpen),
        ("-r", UnaryTestOp::FileIsReadable),
        ("-w", UnaryTestOp::FileIsWritable),
        ("-x", UnaryTestOp::FileIsExecutable),
        ("-u", UnaryTestOp::FileIsSetuid),
        ("-g", UnaryTestOp::FileIsSetgid),
        ("-k", UnaryTestOp::FileIsSticky),
        ("-O", UnaryTestOp::FileIsOwnedByUser),
        ("-G", UnaryTestOp::FileIsOwnedByGroup),
        ("-N", UnaryTestOp::FileModifiedSinceRead),
        ("-z", UnaryTestOp::StringIsEmpty),
        ("-n", UnaryTestOp::StringIsNonEmpty),
        ("-v", UnaryTestOp::VariableIsSet),
        ("-R", UnaryTestOp::VariableIsNameRef),
    ];
    for (op_str, expected_op) in cases {
        let input = format!("[[ {} arg ]]", op_str);
        let expr = parse_test_expr(&input);
        if let BashTestExpr::Unary { op, .. } = &expr {
            assert_eq!(*op, expected_op, "failed for {}", op_str);
        } else {
            panic!("expected Unary for {}, got {:?}", op_str, expr);
        }
    }
}

#[test]
fn test_expr_all_binary_word_ops() {
    // Verify all binary operators that come as Word tokens
    let cases: Vec<(&str, BinaryTestOp)> = vec![
        ("==", BinaryTestOp::StringEquals),
        ("=", BinaryTestOp::StringEquals),
        ("!=", BinaryTestOp::StringNotEquals),
        ("=~", BinaryTestOp::RegexMatch),
        ("-eq", BinaryTestOp::IntEq),
        ("-ne", BinaryTestOp::IntNe),
        ("-lt", BinaryTestOp::IntLt),
        ("-le", BinaryTestOp::IntLe),
        ("-gt", BinaryTestOp::IntGt),
        ("-ge", BinaryTestOp::IntGe),
        ("-nt", BinaryTestOp::FileNewerThan),
        ("-ot", BinaryTestOp::FileOlderThan),
        ("-ef", BinaryTestOp::FileSameDevice),
    ];
    for (op_str, expected_op) in cases {
        let input = format!("[[ a {} b ]]", op_str);
        let expr = parse_test_expr(&input);
        if let BashTestExpr::Binary { op, .. } = &expr {
            assert_eq!(*op, expected_op, "failed for {}", op_str);
        } else {
            panic!("expected Binary for {}, got {:?}", op_str, expr);
        }
    }
}

// ============================================================================
// Arithmetic expression tests (via (( )) and $(( )))
// ============================================================================

/// Helper: parse input in Bash mode (arithmetic_command enabled) and extract ArithExpr.
fn parse_arith_cmd(input: &str) -> ArithExpr {
    let mut opts = ParseOptions::default();
    opts.arithmetic_command = true;
    let prog = thaum::parser::parse_with_options(input, opts).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Compound {
        body: CompoundCommand::BashArithmeticCommand { expression, .. },
        ..
    } = &stmt.expression
    {
        expression.clone()
    } else {
        panic!("expected BashArithmeticCommand, got {:?}", stmt.expression);
    }
}

#[test]
fn arith_number_literal() {
    assert_eq!(parse_arith_cmd("(( 42 ))"), ArithExpr::Number(42));
}

#[test]
fn arith_variable() {
    assert_eq!(
        parse_arith_cmd("(( x + 1 ))"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Variable("x".to_string())),
            op: ArithBinaryOp::Add,
            right: Box::new(ArithExpr::Number(1)),
        }
    );
}

#[test]
fn arith_operator_precedence() {
    // 2 + 3 * 4 → Binary(Add, 2, Binary(Mul, 3, 4))
    assert_eq!(
        parse_arith_cmd("(( 2 + 3 * 4 ))"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(2)),
            op: ArithBinaryOp::Add,
            right: Box::new(ArithExpr::Binary {
                left: Box::new(ArithExpr::Number(3)),
                op: ArithBinaryOp::Mul,
                right: Box::new(ArithExpr::Number(4)),
            }),
        }
    );
}

#[test]
fn arith_assignment() {
    assert_eq!(
        parse_arith_cmd("(( x = 5 ))"),
        ArithExpr::Assignment {
            target: "x".to_string(),
            op: ArithAssignOp::Assign,
            value: Box::new(ArithExpr::Number(5)),
        }
    );
}

#[test]
fn arith_compound_assignment() {
    assert_eq!(
        parse_arith_cmd("(( x += 3 ))"),
        ArithExpr::Assignment {
            target: "x".to_string(),
            op: ArithAssignOp::AddAssign,
            value: Box::new(ArithExpr::Number(3)),
        }
    );
}

#[test]
fn arith_ternary() {
    assert_eq!(
        parse_arith_cmd("(( x > 0 ? 1 : 0 ))"),
        ArithExpr::Ternary {
            condition: Box::new(ArithExpr::Binary {
                left: Box::new(ArithExpr::Variable("x".to_string())),
                op: ArithBinaryOp::Gt,
                right: Box::new(ArithExpr::Number(0)),
            }),
            then_expr: Box::new(ArithExpr::Number(1)),
            else_expr: Box::new(ArithExpr::Number(0)),
        }
    );
}

#[test]
fn arith_pre_increment() {
    assert_eq!(
        parse_arith_cmd("(( ++x ))"),
        ArithExpr::UnaryPrefix {
            op: ArithUnaryOp::Increment,
            operand: Box::new(ArithExpr::Variable("x".to_string())),
        }
    );
}

#[test]
fn arith_post_increment() {
    assert_eq!(
        parse_arith_cmd("(( x++ ))"),
        ArithExpr::UnaryPostfix {
            operand: Box::new(ArithExpr::Variable("x".to_string())),
            op: ArithUnaryOp::Increment,
        }
    );
}

#[test]
fn arith_exponentiation() {
    assert_eq!(
        parse_arith_cmd("(( 2 ** 10 ))"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(2)),
            op: ArithBinaryOp::Exp,
            right: Box::new(ArithExpr::Number(10)),
        }
    );
}

#[test]
fn arith_parenthesized() {
    assert_eq!(
        parse_arith_cmd("(( (1 + 2) * 3 ))"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Group(Box::new(ArithExpr::Binary {
                left: Box::new(ArithExpr::Number(1)),
                op: ArithBinaryOp::Add,
                right: Box::new(ArithExpr::Number(2)),
            }))),
            op: ArithBinaryOp::Mul,
            right: Box::new(ArithExpr::Number(3)),
        }
    );
}

#[test]
fn arith_in_word_context() {
    // echo $(( x + 1 )) — the arithmetic expansion should be parsed
    let prog = parse_with("echo $(( x + 1 ))", Dialect::Bash).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Command(cmd) = &stmt.expression {
        assert_eq!(cmd.arguments.len(), 2);
        if let Argument::Word(w) = &cmd.arguments[1] {
            assert_eq!(
                w.parts,
                vec![Fragment::ArithmeticExpansion(ArithExpr::Binary {
                    left: Box::new(ArithExpr::Variable("x".to_string())),
                    op: ArithBinaryOp::Add,
                    right: Box::new(ArithExpr::Number(1)),
                })]
            );
        } else {
            panic!("expected Word argument");
        }
    } else {
        panic!("expected Command, got {:?}", stmt.expression);
    }
}

// --- arithmetic for loop ---

#[test]
fn bash_arithmetic_for_basic() {
    let mut opts = ParseOptions::default();
    opts.arithmetic_for = true;
    opts.arithmetic_command = true;
    let prog =
        thaum::parser::parse_with_options("for ((i=0; i<10; i++)); do echo $i; done", opts)
            .unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Compound {
        body:
            CompoundCommand::BashArithmeticFor {
                init,
                condition,
                update,
                body,
                ..
            },
        ..
    } = &stmt.expression
    {
        assert!(init.is_some());
        assert!(condition.is_some());
        assert!(update.is_some());
        assert!(!body.is_empty());
    } else {
        panic!("expected BashArithmeticFor, got {:?}", stmt.expression);
    }
}

#[test]
fn bash_arithmetic_for_empty_parts() {
    let mut opts = ParseOptions::default();
    opts.arithmetic_for = true;
    opts.arithmetic_command = true;
    let prog =
        thaum::parser::parse_with_options("for ((;;)); do break; done", opts).unwrap();
    let stmt = &prog.statements[0];
    if let Expression::Compound {
        body:
            CompoundCommand::BashArithmeticFor {
                init,
                condition,
                update,
                ..
            },
        ..
    } = &stmt.expression
    {
        assert!(init.is_none());
        assert!(condition.is_none());
        assert!(update.is_none());
    } else {
        panic!("expected BashArithmeticFor, got {:?}", stmt.expression);
    }
}

#[test]
fn posix_rejects_arithmetic_for() {
    let result = parse("for ((i=0; i<10; i++)); do echo $i; done");
    assert!(result.is_err());
}

// --- parameter expansion inside arithmetic ---

#[test]
fn arith_brace_param_expansion() {
    // `(( ${x} + 1 ))` — simple brace expansion in arithmetic.
    let input = "(( ${x} + 1 ))";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[test]
fn arith_string_length_param() {
    // `(( ${#x} ))` — string length in arithmetic context.
    // Source: /usr/sbin/lvmdump uses `(( ! ${#files[@]} ))`
    let input = "(( ${#x} ))";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[test]
fn arith_array_length() {
    // `(( ${#arr[@]} ))` — array length in arithmetic context.
    // Source: /usr/sbin/lvmdump
    let input = "arr=(a b c); (( ${#arr[@]} ))";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

// --- Array subscripts in arithmetic ---

#[test]
fn arith_array_subscript_basic() {
    parse_with("(( a[0] ))", Dialect::Bash).unwrap();
}

#[test]
fn arith_array_subscript_in_expansion() {
    parse_with("echo $(( a[0] + a[1] ))", Dialect::Bash).unwrap();
}

#[test]
fn arith_array_subscript_assignment() {
    parse_with("(( a[0] = 5 ))", Dialect::Bash).unwrap();
}

#[test]
fn arith_array_subscript_increment() {
    parse_with("(( a[0]++ ))", Dialect::Bash).unwrap();
}

#[test]
fn arith_array_subscript_compound_key() {
    parse_with("(( A[K] = V ))", Dialect::Bash).unwrap();
}

#[test]
fn arith_array_subscript_expr_key() {
    parse_with("(( a[i+1] ))", Dialect::Bash).unwrap();
}

#[test]
fn arith_array_subscript_comma() {
    // Multiple array subscript operations with comma operator
    parse_with("(( a[0]++, ++a[1], a[2]--, --a[3] ))", Dialect::Bash).unwrap();
}

#[test]
fn bracket_glob_word_order() {
    // Verify bracket glob produces correct fragment order: Literal, Glob, Literal(content])
    let prog = parse_with("echo a[0-9]", Dialect::Bash).unwrap();
    if let Expression::Command(cmd) = &prog.statements[0].expression {
        if let Argument::Word(word) = &cmd.arguments[1] {
            let parts = &word.parts;
            assert!(matches!(&parts[0], Fragment::Literal(s) if s == "a"));
            assert!(matches!(&parts[1], Fragment::Glob(GlobChar::BracketOpen)));
            assert!(matches!(&parts[2], Fragment::Literal(s) if s == "0-9]"));
        } else {
            panic!("expected Argument::Word");
        }
    } else {
        panic!("expected Command");
    }
}
