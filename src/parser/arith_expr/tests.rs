//! Arithmetic expression parser tests: literals, variables, all operators,
//! precedence, associativity, assignment, ternary, comma expressions.

use super::*;

testutil::default_labels!(parse);

fn parse_ok(input: &str) -> ArithExpr {
    parse_arith_expr(input).unwrap_or_else(|e| panic!("parse_arith_expr failed for {input:?}: {e}"))
}

// Tokenizer tests -----------------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_decimal_number() {
    assert_eq!(parse_ok("42"), ArithExpr::Number(42));
}

#[testutil::test]
fn lex_hex_number() {
    assert_eq!(parse_ok("0x1F"), ArithExpr::Number(0x1F));
}

#[testutil::test]
fn lex_hex_number_uppercase() {
    assert_eq!(parse_ok("0XFF"), ArithExpr::Number(0xFF));
}

#[testutil::test]
fn lex_octal_number() {
    assert_eq!(parse_ok("077"), ArithExpr::Number(0o77));
}

#[testutil::test]
fn lex_zero() {
    assert_eq!(parse_ok("0"), ArithExpr::Number(0));
}

// Variable tests ------------------------------------------------------------------------------------------------------

#[testutil::test]
fn bare_variable() {
    assert_eq!(parse_ok("x"), ArithExpr::Variable("x".to_string()));
}

#[testutil::test]
fn dollar_variable() {
    assert_eq!(parse_ok("$x"), ArithExpr::Variable("x".to_string()));
}

#[testutil::test]
fn variable_with_underscores() {
    assert_eq!(parse_ok("my_var"), ArithExpr::Variable("my_var".to_string()));
}

#[testutil::test]
fn array_variable() {
    assert_eq!(parse_ok("arr[0]"), ArithExpr::Variable("arr[0]".to_string()));
}

// Binary operator tests -----------------------------------------------------------------------------------------------

#[testutil::test]
fn addition() {
    assert_eq!(
        parse_ok("1 + 2"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(1)),
            op: ArithBinaryOp::Add,
            right: Box::new(ArithExpr::Number(2)),
        }
    );
}

#[testutil::test]
fn subtraction() {
    assert_eq!(
        parse_ok("5 - 3"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(5)),
            op: ArithBinaryOp::Sub,
            right: Box::new(ArithExpr::Number(3)),
        }
    );
}

#[testutil::test]
fn multiplication_binds_tighter_than_addition() {
    // 2 + 3 * 4 → Binary(Add, 2, Binary(Mul, 3, 4))
    assert_eq!(
        parse_ok("2 + 3 * 4"),
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
fn division() {
    assert_eq!(
        parse_ok("10 / 2"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(10)),
            op: ArithBinaryOp::Div,
            right: Box::new(ArithExpr::Number(2)),
        }
    );
}

#[testutil::test]
fn modulo() {
    assert_eq!(
        parse_ok("10 % 3"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(10)),
            op: ArithBinaryOp::Mod,
            right: Box::new(ArithExpr::Number(3)),
        }
    );
}

#[testutil::test]
fn exponentiation() {
    assert_eq!(
        parse_ok("2 ** 10"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(2)),
            op: ArithBinaryOp::Exp,
            right: Box::new(ArithExpr::Number(10)),
        }
    );
}

#[testutil::test]
fn exponentiation_right_associative() {
    // 2 ** 3 ** 2 → Binary(Exp, 2, Binary(Exp, 3, 2))
    assert_eq!(
        parse_ok("2 ** 3 ** 2"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(2)),
            op: ArithBinaryOp::Exp,
            right: Box::new(ArithExpr::Binary {
                left: Box::new(ArithExpr::Number(3)),
                op: ArithBinaryOp::Exp,
                right: Box::new(ArithExpr::Number(2)),
            }),
        }
    );
}

#[testutil::test]
fn shift_left() {
    assert_eq!(
        parse_ok("1 << 4"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(1)),
            op: ArithBinaryOp::ShiftLeft,
            right: Box::new(ArithExpr::Number(4)),
        }
    );
}

#[testutil::test]
fn shift_right() {
    assert_eq!(
        parse_ok("16 >> 2"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(16)),
            op: ArithBinaryOp::ShiftRight,
            right: Box::new(ArithExpr::Number(2)),
        }
    );
}

#[testutil::test]
fn bitwise_and() {
    assert_eq!(
        parse_ok("0xFF & 0x0F"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(0xFF)),
            op: ArithBinaryOp::BitAnd,
            right: Box::new(ArithExpr::Number(0x0F)),
        }
    );
}

#[testutil::test]
fn bitwise_or() {
    assert_eq!(
        parse_ok("1 | 2"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(1)),
            op: ArithBinaryOp::BitOr,
            right: Box::new(ArithExpr::Number(2)),
        }
    );
}

#[testutil::test]
fn bitwise_xor() {
    assert_eq!(
        parse_ok("5 ^ 3"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(5)),
            op: ArithBinaryOp::BitXor,
            right: Box::new(ArithExpr::Number(3)),
        }
    );
}

#[testutil::test]
fn logical_and() {
    assert_eq!(
        parse_ok("1 && 0"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(1)),
            op: ArithBinaryOp::LogAnd,
            right: Box::new(ArithExpr::Number(0)),
        }
    );
}

#[testutil::test]
fn logical_or() {
    assert_eq!(
        parse_ok("0 || 1"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Number(0)),
            op: ArithBinaryOp::LogOr,
            right: Box::new(ArithExpr::Number(1)),
        }
    );
}

#[testutil::test]
fn equality() {
    assert_eq!(
        parse_ok("x == 5"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Variable("x".to_string())),
            op: ArithBinaryOp::Eq,
            right: Box::new(ArithExpr::Number(5)),
        }
    );
}

#[testutil::test]
fn inequality() {
    assert_eq!(
        parse_ok("x != 5"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Variable("x".to_string())),
            op: ArithBinaryOp::Ne,
            right: Box::new(ArithExpr::Number(5)),
        }
    );
}

#[testutil::test]
fn comparisons() {
    assert!(matches!(
        parse_ok("a < b"),
        ArithExpr::Binary {
            op: ArithBinaryOp::Lt,
            ..
        }
    ));
    assert!(matches!(
        parse_ok("a <= b"),
        ArithExpr::Binary {
            op: ArithBinaryOp::Le,
            ..
        }
    ));
    assert!(matches!(
        parse_ok("a > b"),
        ArithExpr::Binary {
            op: ArithBinaryOp::Gt,
            ..
        }
    ));
    assert!(matches!(
        parse_ok("a >= b"),
        ArithExpr::Binary {
            op: ArithBinaryOp::Ge,
            ..
        }
    ));
}

// Unary operator tests ------------------------------------------------------------------------------------------------

#[testutil::test]
fn unary_negate() {
    assert_eq!(
        parse_ok("-x"),
        ArithExpr::UnaryPrefix {
            op: ArithUnaryOp::Negate,
            operand: Box::new(ArithExpr::Variable("x".to_string())),
        }
    );
}

#[testutil::test]
fn unary_plus() {
    assert_eq!(
        parse_ok("+x"),
        ArithExpr::UnaryPrefix {
            op: ArithUnaryOp::Plus,
            operand: Box::new(ArithExpr::Variable("x".to_string())),
        }
    );
}

#[testutil::test]
fn logical_not() {
    assert_eq!(
        parse_ok("!x"),
        ArithExpr::UnaryPrefix {
            op: ArithUnaryOp::LogNot,
            operand: Box::new(ArithExpr::Variable("x".to_string())),
        }
    );
}

#[testutil::test]
fn bitwise_not() {
    assert_eq!(
        parse_ok("~x"),
        ArithExpr::UnaryPrefix {
            op: ArithUnaryOp::BitNot,
            operand: Box::new(ArithExpr::Variable("x".to_string())),
        }
    );
}

#[testutil::test]
fn pre_increment() {
    assert_eq!(
        parse_ok("++x"),
        ArithExpr::UnaryPrefix {
            op: ArithUnaryOp::Increment,
            operand: Box::new(ArithExpr::Variable("x".to_string())),
        }
    );
}

#[testutil::test]
fn pre_decrement() {
    assert_eq!(
        parse_ok("--x"),
        ArithExpr::UnaryPrefix {
            op: ArithUnaryOp::Decrement,
            operand: Box::new(ArithExpr::Variable("x".to_string())),
        }
    );
}

#[testutil::test]
fn post_increment() {
    assert_eq!(
        parse_ok("x++"),
        ArithExpr::UnaryPostfix {
            operand: Box::new(ArithExpr::Variable("x".to_string())),
            op: ArithUnaryOp::Increment,
        }
    );
}

#[testutil::test]
fn post_decrement() {
    assert_eq!(
        parse_ok("x--"),
        ArithExpr::UnaryPostfix {
            operand: Box::new(ArithExpr::Variable("x".to_string())),
            op: ArithUnaryOp::Decrement,
        }
    );
}

// Assignment tests ----------------------------------------------------------------------------------------------------

#[testutil::test]
fn simple_assignment() {
    assert_eq!(
        parse_ok("x = 5"),
        ArithExpr::Assignment {
            target: "x".to_string(),
            op: ArithAssignOp::Assign,
            value: Box::new(ArithExpr::Number(5)),
        }
    );
}

#[testutil::test]
fn compound_add_assign() {
    assert_eq!(
        parse_ok("x += 3"),
        ArithExpr::Assignment {
            target: "x".to_string(),
            op: ArithAssignOp::AddAssign,
            value: Box::new(ArithExpr::Number(3)),
        }
    );
}

#[testutil::test]
fn compound_sub_assign() {
    assert_eq!(
        parse_ok("x -= 1"),
        ArithExpr::Assignment {
            target: "x".to_string(),
            op: ArithAssignOp::SubAssign,
            value: Box::new(ArithExpr::Number(1)),
        }
    );
}

#[testutil::test]
fn compound_mul_assign() {
    assert_eq!(
        parse_ok("x *= 2"),
        ArithExpr::Assignment {
            target: "x".to_string(),
            op: ArithAssignOp::MulAssign,
            value: Box::new(ArithExpr::Number(2)),
        }
    );
}

#[testutil::test]
fn compound_all_assign_ops() {
    let cases: Vec<(&str, ArithAssignOp)> = vec![
        ("x = 1", ArithAssignOp::Assign),
        ("x += 1", ArithAssignOp::AddAssign),
        ("x -= 1", ArithAssignOp::SubAssign),
        ("x *= 1", ArithAssignOp::MulAssign),
        ("x /= 1", ArithAssignOp::DivAssign),
        ("x %= 1", ArithAssignOp::ModAssign),
        ("x <<= 1", ArithAssignOp::ShiftLeftAssign),
        ("x >>= 1", ArithAssignOp::ShiftRightAssign),
        ("x &= 1", ArithAssignOp::BitAndAssign),
        ("x |= 1", ArithAssignOp::BitOrAssign),
        ("x ^= 1", ArithAssignOp::BitXorAssign),
    ];
    for (input, expected_op) in cases {
        let expr = parse_ok(input);
        if let ArithExpr::Assignment { op, .. } = &expr {
            assert_eq!(*op, expected_op, "failed for {input}");
        } else {
            panic!("expected Assignment for {input}, got {expr:?}");
        }
    }
}

#[testutil::test]
fn assignment_right_associative() {
    // x = y = 5 → Assignment(x, Assign, Assignment(y, Assign, 5))
    let expr = parse_ok("x = y = 5");
    if let ArithExpr::Assignment { target, value, .. } = &expr {
        assert_eq!(target, "x");
        assert!(matches!(value.as_ref(), ArithExpr::Assignment { .. }));
    } else {
        panic!("expected Assignment, got {expr:?}");
    }
}

// Ternary test --------------------------------------------------------------------------------------------------------

#[testutil::test]
fn ternary_expression() {
    assert_eq!(
        parse_ok("x > 0 ? 1 : 0"),
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

// Grouping ------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn parenthesized_expression() {
    assert_eq!(
        parse_ok("(1 + 2) * 3"),
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

// Comma ---------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn comma_expression() {
    assert_eq!(
        parse_ok("x = 1, y = 2"),
        ArithExpr::Comma {
            left: Box::new(ArithExpr::Assignment {
                target: "x".to_string(),
                op: ArithAssignOp::Assign,
                value: Box::new(ArithExpr::Number(1)),
            }),
            right: Box::new(ArithExpr::Assignment {
                target: "y".to_string(),
                op: ArithAssignOp::Assign,
                value: Box::new(ArithExpr::Number(2)),
            }),
        }
    );
}

// Mixed expression ----------------------------------------------------------------------------------------------------

#[testutil::test]
fn variable_plus_number() {
    assert_eq!(
        parse_ok("x + 1"),
        ArithExpr::Binary {
            left: Box::new(ArithExpr::Variable("x".to_string())),
            op: ArithBinaryOp::Add,
            right: Box::new(ArithExpr::Number(1)),
        }
    );
}

#[testutil::test]
fn complex_precedence() {
    // a + b * c - d → Sub(Add(a, Mul(b, c)), d)
    let expr = parse_ok("a + b * c - d");
    if let ArithExpr::Binary {
        op: ArithBinaryOp::Sub,
        left,
        ..
    } = &expr
    {
        assert!(matches!(
            left.as_ref(),
            ArithExpr::Binary {
                op: ArithBinaryOp::Add,
                ..
            }
        ));
    } else {
        panic!("expected Sub at top, got {expr:?}");
    }
}

// Error cases ---------------------------------------------------------------------------------------------------------

#[testutil::test]
fn empty_expression_is_zero() {
    assert_eq!(parse_arith_expr("").unwrap(), ArithExpr::Number(0));
}

#[testutil::test]
fn whitespace_only_expression_is_zero() {
    assert_eq!(parse_arith_expr("   ").unwrap(), ArithExpr::Number(0));
}

#[testutil::test]
fn error_unclosed_paren() {
    assert!(parse_arith_expr("(1 + 2").is_err());
}

#[testutil::test]
fn error_unexpected_token() {
    assert!(parse_arith_expr(")").is_err());
}

#[testutil::test]
fn error_trailing_operator() {
    assert!(parse_arith_expr("1 +").is_err());
}

#[testutil::test]
fn error_assignment_to_number() {
    assert!(parse_arith_expr("5 = 3").is_err());
}
