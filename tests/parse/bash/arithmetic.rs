use thaum::ast::*;
use thaum::{parse, parse_with, Dialect, ShellOptions};

// Arithmetic expression tests (via (( )) and $(( ))) ------------------------------------------------------------------

/// Helper: parse input in Bash mode (arithmetic_command enabled) and extract ArithExpr.
fn parse_arith_cmd(input: &str) -> ArithExpr {
    let opts = ShellOptions {
        arithmetic_command: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options(input, opts).unwrap();
    let stmt = &prog.lines[0][0];
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

#[testutil::test]
fn arith_number_literal() {
    assert_eq!(parse_arith_cmd("(( 42 ))"), ArithExpr::Number(42));
}

#[testutil::test]
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

#[testutil::test]
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

#[testutil::test]
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

#[testutil::test]
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

#[testutil::test]
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

#[testutil::test]
fn arith_pre_increment() {
    assert_eq!(
        parse_arith_cmd("(( ++x ))"),
        ArithExpr::UnaryPrefix {
            op: ArithUnaryOp::Increment,
            operand: Box::new(ArithExpr::Variable("x".to_string())),
        }
    );
}

#[testutil::test]
fn arith_post_increment() {
    assert_eq!(
        parse_arith_cmd("(( x++ ))"),
        ArithExpr::UnaryPostfix {
            operand: Box::new(ArithExpr::Variable("x".to_string())),
            op: ArithUnaryOp::Increment,
        }
    );
}

#[testutil::test]
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

#[testutil::test]
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

#[testutil::test]
fn arith_in_word_context() {
    // echo $(( x + 1 )) — the arithmetic expansion should be parsed
    let prog = parse_with("echo $(( x + 1 ))", Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
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

// arithmetic for loop -------------------------------------------------------------------------------------------------

#[testutil::test]
fn bash_arithmetic_for_basic() {
    let opts = ShellOptions {
        arithmetic_for: true,
        arithmetic_command: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("for ((i=0; i<10; i++)); do echo $i; done", opts).unwrap();
    let stmt = &prog.lines[0][0];
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

#[testutil::test]
fn bash_arithmetic_for_empty_parts() {
    let opts = ShellOptions {
        arithmetic_for: true,
        arithmetic_command: true,
        ..Default::default()
    };
    let prog = thaum::parser::parse_with_options("for ((;;)); do break; done", opts).unwrap();
    let stmt = &prog.lines[0][0];
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

#[testutil::test]
fn posix_rejects_arithmetic_for() {
    let result = parse("for ((i=0; i<10; i++)); do echo $i; done");
    assert!(result.is_err());
}

// parameter expansion inside arithmetic -------------------------------------------------------------------------------

#[testutil::test]
fn arith_brace_param_expansion() {
    // `(( ${x} + 1 ))` — simple brace expansion in arithmetic.
    let input = "(( ${x} + 1 ))";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[testutil::test]
fn arith_string_length_param() {
    // `(( ${#x} ))` — string length in arithmetic context.
    // Source: /usr/sbin/lvmdump uses `(( ! ${#files[@]} ))`
    let input = "(( ${#x} ))";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[testutil::test]
fn arith_array_length() {
    // `(( ${#arr[@]} ))` — array length in arithmetic context.
    // Source: /usr/sbin/lvmdump
    let input = "arr=(a b c); (( ${#arr[@]} ))";
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

// Array subscripts in arithmetic --------------------------------------------------------------------------------------

#[testutil::test]
fn arith_array_subscript_basic() {
    parse_with("(( a[0] ))", Dialect::Bash).unwrap();
}

#[testutil::test]
fn arith_array_subscript_in_expansion() {
    parse_with("echo $(( a[0] + a[1] ))", Dialect::Bash).unwrap();
}

#[testutil::test]
fn arith_array_subscript_assignment() {
    parse_with("(( a[0] = 5 ))", Dialect::Bash).unwrap();
}

#[testutil::test]
fn arith_array_subscript_increment() {
    parse_with("(( a[0]++ ))", Dialect::Bash).unwrap();
}

#[testutil::test]
fn arith_array_subscript_compound_key() {
    parse_with("(( A[K] = V ))", Dialect::Bash).unwrap();
}

#[testutil::test]
fn arith_array_subscript_expr_key() {
    parse_with("(( a[i+1] ))", Dialect::Bash).unwrap();
}

#[testutil::test]
fn arith_array_subscript_comma() {
    // Multiple array subscript operations with comma operator
    parse_with("(( a[0]++, ++a[1], a[2]--, --a[3] ))", Dialect::Bash).unwrap();
}

#[testutil::test]
fn bracket_glob_word_order() {
    // Verify bracket glob produces correct fragment order: Literal, Glob, Literal(content])
    let prog = parse_with("echo a[0-9]", Dialect::Bash).unwrap();
    if let Expression::Command(cmd) = &prog.lines[0][0].expression {
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

// << inside (( )) is left-shift, not heredoc --------------------------------------------------------------------------

#[testutil::test]
fn arith_left_shift_not_heredoc() {
    // << inside (( )) is left-shift, not a heredoc operator.
    let prog = parse_with("(( 1 << 32 ))\necho ok", Dialect::Bash).unwrap();
    assert_eq!(prog.lines.len(), 2);
    assert!(matches!(
        &prog.lines[0][0].expression,
        Expression::Compound {
            body: CompoundCommand::BashArithmeticCommand { .. },
            ..
        }
    ));
}

#[testutil::test]
fn for_arith_left_shift_not_heredoc() {
    // << inside for (( )) is left-shift, not a heredoc.
    let input = "x=0\n\nfor ((i = 1 << 32; i; ++i)); do\nbreak\ndone";
    parse_with(input, Dialect::Bash).unwrap();
}

#[testutil::test]
fn double_paren_subshell_not_arithmetic() {
    // ((/path/cmd ...)) — (( followed by / means subshell-of-subshell, not arithmetic.
    // The speculative arithmetic attempt fails (no )) found), so it falls back to subshell.
    let prog = parse_with("((/usr/bin/cat </dev/zero; echo hi) | true)", Dialect::Bash).unwrap();
    assert!(matches!(
        &prog.lines[0][0].expression,
        Expression::Compound {
            body: CompoundCommand::Subshell { .. },
            ..
        }
    ));
}

// Arithmetic features: (( )) must parse as BashArithmeticCommand, not Subshell ----------------------------------------

#[testutil::test]
fn arith_empty_expression() {
    // (( )) is valid bash — evaluates to 0 (exit status 1).
    let expr = parse_arith_cmd("(( ))");
    assert_eq!(expr, ArithExpr::Number(0));
}

#[testutil::test]
fn arith_single_quoted_value() {
    // Single-quoted string as rhs inside (( )).
    parse_arith_cmd("(( A['y'] = 'y' ))");
}

#[testutil::test]
fn arith_command_sub() {
    // $() inside (( )).
    parse_arith_cmd("(( a = $(echo 1) + 2 ))");
}

#[testutil::test]
fn arith_dollar_positional() {
    // $N (positional parameter) inside (( )).
    parse_arith_cmd("(( A[$key] += $2 ))");
}

#[testutil::test]
fn arith_literal_subscript() {
    // 1[2] should parse as arithmetic, not fall back to subshell.
    parse_arith_cmd("(( 1[2] = 3 ))");
}

#[testutil::test]
fn arith_redirect_after_dparen() {
    // Redirect after (( )) with $() inside.
    let prog = parse_with("(( a = $(echo 42) + 10 )) 2>/dev/null", Dialect::Bash).unwrap();
    assert!(matches!(
        &prog.lines[0][0].expression,
        Expression::Compound {
            body: CompoundCommand::BashArithmeticCommand { .. },
            ..
        }
    ));
}

// [[ ]] edge cases ----------------------------------------------------------------------------------------------------

#[testutil::test]
fn double_bracket_close_as_literal_word() {
    // ]] outside [[ ]] is a regular word, not BashDblRBracket.
    let prog = parse_with("dbracket=[[\n$dbracket foo == foo ]]", Dialect::Bash).unwrap();
    assert!(prog.lines.len() >= 2);
}

#[testutil::test]
fn glob_posix_char_class_not_double_bracket() {
    // [[:punct:]] is a POSIX character class in a glob, not [[ ]].
    let prog = parse_with("echo *.[[:punct:]]", Dialect::Bash).unwrap();
    assert_eq!(prog.lines[0].len(), 1);
}

#[testutil::test]
fn regex_with_parens_in_grouped_double_bracket() {
    // [[ (foo =~ bar) ]] — grouped test expression with =~ inside.
    let prog = parse_with("[[ (foo =~ bar) ]]", Dialect::Bash).unwrap();
    if let Expression::Compound {
        body: CompoundCommand::BashDoubleBracket { expression, .. },
        ..
    } = &prog.lines[0][0].expression
    {
        assert!(matches!(expression, BashTestExpr::Group(_)));
    } else {
        panic!("expected BashDoubleBracket");
    }
}

#[testutil::test]
fn glob_bracket_with_quoted_close() {
    // ] inside quotes doesn't close a bracket expression.
    // bash treats [hello"]" as the literal word [hello] (no glob match).
    let prog = parse_with("echo [hello\"]\"", Dialect::Bash).unwrap();
    assert_eq!(prog.lines[0].len(), 1);
}
