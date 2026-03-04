use thaum::ast::*;
use thaum::{parse_with, Dialect, ShellOptions};

// [[ ]] test expression parsing ---------------------------------------------------------------------------------------

/// Helper: parse input in Bash mode and extract the BashTestExpr.
fn parse_test_expr(input: &str) -> BashTestExpr {
    let prog = parse_with(input, Dialect::Bash).unwrap();
    let stmt = &prog.lines[0][0];
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

#[skuld::test]
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

#[skuld::test]
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

#[skuld::test]
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

#[skuld::test]
fn test_expr_binary_string_equals() {
    let expr = parse_test_expr(r#"[[ $a == hello ]]"#);
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringEquals);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_binary_string_not_equals() {
    let expr = parse_test_expr("[[ $a != $b ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringNotEquals);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_binary_int_eq() {
    let expr = parse_test_expr("[[ $x -eq 0 ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::IntEq);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_binary_int_lt() {
    let expr = parse_test_expr("[[ $x -lt 10 ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::IntLt);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_binary_less_than() {
    // < and > are lexed as RedirectFromFile / RedirectToFile tokens
    let expr = parse_test_expr("[[ $a < $b ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringLessThan);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_binary_greater_than() {
    let expr = parse_test_expr("[[ $a > $b ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringGreaterThan);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_binary_regex_match() {
    let expr = parse_test_expr("[[ $str =~ ^[0-9]+$ ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::RegexMatch);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_regex_with_unquoted_parens() {
    // Parentheses in a =~ regex are capturing groups, not shell syntax.
    // Source: /usr/bin/socat-chain.sh
    let input = r#"[[ "$x" =~ ^([^:]*):([^:]*) ]]"#;
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[skuld::test]
fn test_expr_regex_with_alternation_in_parens() {
    // Pipe inside regex parens is alternation, not a shell pipe.
    // Source: /usr/lib/snapd/complete.sh
    let input = r#"[[ "${BASH_SOURCE[0]}" =~ ^(/var/lib|/usr/share)/completions/ ]]"#;
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[skuld::test]
fn test_expr_regex_with_escaped_parens() {
    // Mixed escaped and unescaped parens in regex.
    // Source: /usr/local/go/.../mkerrors.bash
    let input = r#"[[ $line =~ ^#define\ +([A-Z]+)\ +\(\(([A-Z]+)\)([0-9]+)\) ]]"#;
    assert!(parse_with(input, Dialect::Bash).is_ok());
}

#[skuld::test]
fn test_expr_binary_file_newer() {
    let expr = parse_test_expr("[[ a.txt -nt b.txt ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::FileNewerThan);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
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
        panic!("expected Or, got {expr:?}");
    }
}

#[skuld::test]
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
        panic!("expected Not, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_double_not() {
    // [[ ! ! -f foo ]] → Not(Not(Unary(-f, foo)))
    let expr = parse_test_expr("[[ ! ! -f foo ]]");
    if let BashTestExpr::Not(inner) = &expr {
        assert!(matches!(inner.as_ref(), BashTestExpr::Not(_)));
    } else {
        panic!("expected Not, got {expr:?}");
    }
}

#[skuld::test]
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
        panic!("expected Group, got {expr:?}");
    }
}

// [[ ]] multi-line and edge cases -------------------------------------------------------------------------------------

#[skuld::test]
fn dbracket_multiline_and() {
    // [[ over multiple lines with &&
    let expr = parse_test_expr("[[ foo == foo\n&& bar == bar\n]]");
    assert!(matches!(expr, BashTestExpr::And { .. }));
}

#[skuld::test]
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

#[skuld::test]
fn dbracket_string_gt_no_space() {
    // [[ b>a ]] — string > comparison with no spaces around >
    let expr = parse_test_expr("[[ b>a ]]");
    assert!(matches!(
        expr,
        BashTestExpr::Binary {
            op: BinaryTestOp::StringGreaterThan,
            ..
        }
    ));
}

#[skuld::test]
fn dbracket_string_lt_no_space() {
    // [[ a<b ]] — string < comparison
    let expr = parse_test_expr("[[ a<b ]]");
    assert!(matches!(
        expr,
        BashTestExpr::Binary {
            op: BinaryTestOp::StringLessThan,
            ..
        }
    ));
}

#[skuld::test]
fn test_expr_bare_word() {
    // [[ word ]] → implicit -n test
    let expr = parse_test_expr("[[ hello ]]");
    assert!(matches!(expr, BashTestExpr::Word(_)));
}

#[skuld::test]
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
        panic!("expected Or, got {expr:?}");
    }
}

#[skuld::test]
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
        panic!("expected And, got {expr:?}");
    }
}

#[skuld::test]
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
        panic!("expected And, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_binary_eq_single_equals() {
    // [[ $a = pattern ]] — single = is the same as ==
    let expr = parse_test_expr("[[ $a = hello ]]");
    if let BashTestExpr::Binary { op, .. } = &expr {
        assert_eq!(*op, BinaryTestOp::StringEquals);
    } else {
        panic!("expected Binary, got {expr:?}");
    }
}

#[skuld::test]
fn test_expr_unclosed_double_bracket_is_error() {
    let opts = ShellOptions {
        double_brackets: true,
        ..Default::default()
    };
    let result = thaum::parser::parse_with_options("[[ -f foo", opts);
    assert!(result.is_err());
}

#[skuld::test]
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
        panic!("expected And, got {expr:?}");
    }
}

#[skuld::test]
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
        panic!("expected Or, got {expr:?}");
    }
}

#[skuld::test]
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
        let input = format!("[[ {op_str} arg ]]");
        let expr = parse_test_expr(&input);
        if let BashTestExpr::Unary { op, .. } = &expr {
            assert_eq!(*op, expected_op, "failed for {op_str}");
        } else {
            panic!("expected Unary for {op_str}, got {expr:?}");
        }
    }
}

#[skuld::test]
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
        let input = format!("[[ a {op_str} b ]]");
        let expr = parse_test_expr(&input);
        if let BashTestExpr::Binary { op, .. } = &expr {
            assert_eq!(*op, expected_op, "failed for {op_str}");
        } else {
            panic!("expected Binary for {op_str}, got {expr:?}");
        }
    }
}
