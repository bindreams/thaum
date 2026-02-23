use super::*;
use crate::ast::{ArithAssignOp, ArithBinaryOp, ArithExpr, ArithUnaryOp};
use crate::exec::environment::Environment;

// Helpers -------------------------------------------------------------------------------------------------------------

fn num(n: i64) -> ArithExpr {
    ArithExpr::Number(n)
}

fn var(name: &str) -> ArithExpr {
    ArithExpr::Variable(name.to_string())
}

fn binary(left: ArithExpr, op: ArithBinaryOp, right: ArithExpr) -> ArithExpr {
    ArithExpr::Binary {
        left: Box::new(left),
        op,
        right: Box::new(right),
    }
}

fn prefix(op: ArithUnaryOp, operand: ArithExpr) -> ArithExpr {
    ArithExpr::UnaryPrefix {
        op,
        operand: Box::new(operand),
    }
}

fn postfix(operand: ArithExpr, op: ArithUnaryOp) -> ArithExpr {
    ArithExpr::UnaryPostfix {
        operand: Box::new(operand),
        op,
    }
}

fn assign(target: &str, op: ArithAssignOp, value: ArithExpr) -> ArithExpr {
    ArithExpr::Assignment {
        target: target.to_string(),
        op,
        value: Box::new(value),
    }
}

fn ternary(cond: ArithExpr, then_expr: ArithExpr, else_expr: ArithExpr) -> ArithExpr {
    ArithExpr::Ternary {
        condition: Box::new(cond),
        then_expr: Box::new(then_expr),
        else_expr: Box::new(else_expr),
    }
}

fn comma(left: ArithExpr, right: ArithExpr) -> ArithExpr {
    ArithExpr::Comma {
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn group(inner: ArithExpr) -> ArithExpr {
    ArithExpr::Group(Box::new(inner))
}

// Number literals -----------------------------------------------------------------------------------------------------

#[test]
fn eval_number_positive() {
    let mut env = Environment::new();
    assert_eq!(evaluate_arith_expr(&num(42), &mut env).unwrap(), 42);
}

#[test]
fn eval_number_negative() {
    let mut env = Environment::new();
    assert_eq!(evaluate_arith_expr(&num(-7), &mut env).unwrap(), -7);
}

#[test]
fn eval_number_zero() {
    let mut env = Environment::new();
    assert_eq!(evaluate_arith_expr(&num(0), &mut env).unwrap(), 0);
}

// Variable lookup -----------------------------------------------------------------------------------------------------

#[test]
fn eval_variable_unset_is_zero() {
    let mut env = Environment::new();
    assert_eq!(evaluate_arith_expr(&var("x"), &mut env).unwrap(), 0);
}

#[test]
fn eval_variable_empty_is_zero() {
    let mut env = Environment::new();
    env.set_var("x", "").unwrap();
    assert_eq!(evaluate_arith_expr(&var("x"), &mut env).unwrap(), 0);
}

#[test]
fn eval_variable_numeric() {
    let mut env = Environment::new();
    env.set_var("x", "42").unwrap();
    assert_eq!(evaluate_arith_expr(&var("x"), &mut env).unwrap(), 42);
}

#[test]
fn eval_variable_negative() {
    let mut env = Environment::new();
    env.set_var("x", "-10").unwrap();
    assert_eq!(evaluate_arith_expr(&var("x"), &mut env).unwrap(), -10);
}

#[test]
fn eval_variable_non_numeric_is_error() {
    let mut env = Environment::new();
    env.set_var("x", "abc").unwrap();
    let err = evaluate_arith_expr(&var("x"), &mut env).unwrap_err();
    assert!(matches!(err, ExecError::InvalidNumber(_, _)));
}

// Basic arithmetic ----------------------------------------------------------------------------------------------------

#[test]
fn eval_add() {
    let mut env = Environment::new();
    let expr = binary(num(3), ArithBinaryOp::Add, num(4));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 7);
}

#[test]
fn eval_sub() {
    let mut env = Environment::new();
    let expr = binary(num(10), ArithBinaryOp::Sub, num(3));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 7);
}

#[test]
fn eval_mul() {
    let mut env = Environment::new();
    let expr = binary(num(6), ArithBinaryOp::Mul, num(7));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 42);
}

#[test]
fn eval_div() {
    let mut env = Environment::new();
    let expr = binary(num(15), ArithBinaryOp::Div, num(4));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 3);
}

#[test]
fn eval_mod() {
    let mut env = Environment::new();
    let expr = binary(num(17), ArithBinaryOp::Mod, num(5));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 2);
}

#[test]
fn eval_div_by_zero() {
    let mut env = Environment::new();
    let expr = binary(num(1), ArithBinaryOp::Div, num(0));
    let err = evaluate_arith_expr(&expr, &mut env).unwrap_err();
    assert!(matches!(err, ExecError::DivisionByZero));
}

#[test]
fn eval_mod_by_zero() {
    let mut env = Environment::new();
    let expr = binary(num(1), ArithBinaryOp::Mod, num(0));
    let err = evaluate_arith_expr(&expr, &mut env).unwrap_err();
    assert!(matches!(err, ExecError::DivisionByZero));
}

#[test]
fn eval_add_wrapping_overflow() {
    let mut env = Environment::new();
    let expr = binary(num(i64::MAX), ArithBinaryOp::Add, num(1));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), i64::MIN);
}

// Exponentiation ------------------------------------------------------------------------------------------------------

#[test]
fn eval_exp_positive() {
    let mut env = Environment::new();
    let expr = binary(num(2), ArithBinaryOp::Exp, num(10));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 1024);
}

#[test]
fn eval_exp_zero() {
    let mut env = Environment::new();
    let expr = binary(num(5), ArithBinaryOp::Exp, num(0));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 1);
}

#[test]
fn eval_exp_negative_is_error() {
    let mut env = Environment::new();
    let expr = binary(num(2), ArithBinaryOp::Exp, num(-1));
    let err = evaluate_arith_expr(&expr, &mut env).unwrap_err();
    assert!(matches!(err, ExecError::InvalidNumber(_, _)));
}

#[test]
fn eval_exp_one() {
    let mut env = Environment::new();
    let expr = binary(num(1), ArithBinaryOp::Exp, num(1000));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 1);
}

// Comparison ----------------------------------------------------------------------------------------------------------

#[test]
fn eval_eq_true() {
    let mut env = Environment::new();
    let expr = binary(num(5), ArithBinaryOp::Eq, num(5));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 1);
}

#[test]
fn eval_eq_false() {
    let mut env = Environment::new();
    let expr = binary(num(5), ArithBinaryOp::Eq, num(3));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 0);
}

#[test]
fn eval_ne() {
    let mut env = Environment::new();
    assert_eq!(
        evaluate_arith_expr(&binary(num(5), ArithBinaryOp::Ne, num(3)), &mut env).unwrap(),
        1
    );
    assert_eq!(
        evaluate_arith_expr(&binary(num(5), ArithBinaryOp::Ne, num(5)), &mut env).unwrap(),
        0
    );
}

#[test]
fn eval_lt() {
    let mut env = Environment::new();
    assert_eq!(
        evaluate_arith_expr(&binary(num(3), ArithBinaryOp::Lt, num(5)), &mut env).unwrap(),
        1
    );
    assert_eq!(
        evaluate_arith_expr(&binary(num(5), ArithBinaryOp::Lt, num(5)), &mut env).unwrap(),
        0
    );
}

#[test]
fn eval_le() {
    let mut env = Environment::new();
    assert_eq!(
        evaluate_arith_expr(&binary(num(5), ArithBinaryOp::Le, num(5)), &mut env).unwrap(),
        1
    );
    assert_eq!(
        evaluate_arith_expr(&binary(num(6), ArithBinaryOp::Le, num(5)), &mut env).unwrap(),
        0
    );
}

#[test]
fn eval_gt() {
    let mut env = Environment::new();
    assert_eq!(
        evaluate_arith_expr(&binary(num(5), ArithBinaryOp::Gt, num(3)), &mut env).unwrap(),
        1
    );
    assert_eq!(
        evaluate_arith_expr(&binary(num(3), ArithBinaryOp::Gt, num(5)), &mut env).unwrap(),
        0
    );
}

#[test]
fn eval_ge() {
    let mut env = Environment::new();
    assert_eq!(
        evaluate_arith_expr(&binary(num(5), ArithBinaryOp::Ge, num(5)), &mut env).unwrap(),
        1
    );
    assert_eq!(
        evaluate_arith_expr(&binary(num(4), ArithBinaryOp::Ge, num(5)), &mut env).unwrap(),
        0
    );
}

// Logical operators with short-circuit --------------------------------------------------------------------------------

#[test]
fn eval_log_and_true() {
    let mut env = Environment::new();
    let expr = binary(num(1), ArithBinaryOp::LogAnd, num(1));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 1);
}

#[test]
fn eval_log_and_short_circuit() {
    let mut env = Environment::new();
    // 0 && (x=5) should not assign x
    let expr = binary(
        num(0),
        ArithBinaryOp::LogAnd,
        assign("x", ArithAssignOp::Assign, num(5)),
    );
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 0);
    assert_eq!(env.get_var("x"), None);
}

#[test]
fn eval_log_or_false() {
    let mut env = Environment::new();
    let expr = binary(num(0), ArithBinaryOp::LogOr, num(0));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 0);
}

#[test]
fn eval_log_or_short_circuit() {
    let mut env = Environment::new();
    // 1 || (x=5) should not assign x
    let expr = binary(num(1), ArithBinaryOp::LogOr, assign("x", ArithAssignOp::Assign, num(5)));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 1);
    assert_eq!(env.get_var("x"), None);
}

// Bitwise operators ---------------------------------------------------------------------------------------------------

#[test]
fn eval_bit_and() {
    let mut env = Environment::new();
    let expr = binary(num(0b1100), ArithBinaryOp::BitAnd, num(0b1010));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 0b1000);
}

#[test]
fn eval_bit_or() {
    let mut env = Environment::new();
    let expr = binary(num(0b1100), ArithBinaryOp::BitOr, num(0b1010));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 0b1110);
}

#[test]
fn eval_bit_xor() {
    let mut env = Environment::new();
    let expr = binary(num(0b1100), ArithBinaryOp::BitXor, num(0b1010));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 0b0110);
}

#[test]
fn eval_shift_left() {
    let mut env = Environment::new();
    let expr = binary(num(1), ArithBinaryOp::ShiftLeft, num(4));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 16);
}

#[test]
fn eval_shift_right() {
    let mut env = Environment::new();
    let expr = binary(num(16), ArithBinaryOp::ShiftRight, num(2));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 4);
}

// Unary prefix --------------------------------------------------------------------------------------------------------

#[test]
fn eval_negate() {
    let mut env = Environment::new();
    let expr = prefix(ArithUnaryOp::Negate, num(5));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), -5);
}

#[test]
fn eval_unary_plus() {
    let mut env = Environment::new();
    let expr = prefix(ArithUnaryOp::Plus, num(5));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 5);
}

#[test]
fn eval_log_not_zero() {
    let mut env = Environment::new();
    let expr = prefix(ArithUnaryOp::LogNot, num(0));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 1);
}

#[test]
fn eval_log_not_nonzero() {
    let mut env = Environment::new();
    let expr = prefix(ArithUnaryOp::LogNot, num(42));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 0);
}

#[test]
fn eval_bit_not() {
    let mut env = Environment::new();
    let expr = prefix(ArithUnaryOp::BitNot, num(0));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), -1);
}

#[test]
fn eval_prefix_increment() {
    let mut env = Environment::new();
    env.set_var("x", "5").unwrap();
    let expr = prefix(ArithUnaryOp::Increment, var("x"));
    // ++x returns new value
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 6);
    assert_eq!(env.get_var("x"), Some("6"));
}

#[test]
fn eval_prefix_decrement() {
    let mut env = Environment::new();
    env.set_var("x", "5").unwrap();
    let expr = prefix(ArithUnaryOp::Decrement, var("x"));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 4);
    assert_eq!(env.get_var("x"), Some("4"));
}

// Unary postfix -------------------------------------------------------------------------------------------------------

#[test]
fn eval_postfix_increment() {
    let mut env = Environment::new();
    env.set_var("x", "5").unwrap();
    let expr = postfix(var("x"), ArithUnaryOp::Increment);
    // x++ returns old value
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 5);
    assert_eq!(env.get_var("x"), Some("6"));
}

#[test]
fn eval_postfix_decrement() {
    let mut env = Environment::new();
    env.set_var("x", "5").unwrap();
    let expr = postfix(var("x"), ArithUnaryOp::Decrement);
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 5);
    assert_eq!(env.get_var("x"), Some("4"));
}

// Ternary -------------------------------------------------------------------------------------------------------------

#[test]
fn eval_ternary_true_branch() {
    let mut env = Environment::new();
    let expr = ternary(num(1), num(10), num(20));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 10);
}

#[test]
fn eval_ternary_false_branch() {
    let mut env = Environment::new();
    let expr = ternary(num(0), num(10), num(20));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 20);
}

#[test]
fn eval_ternary_lazy() {
    let mut env = Environment::new();
    // 1 ? (x=10) : (x=20) should only evaluate (x=10)
    let expr = ternary(
        num(1),
        assign("x", ArithAssignOp::Assign, num(10)),
        assign("x", ArithAssignOp::Assign, num(20)),
    );
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 10);
    assert_eq!(env.get_var("x"), Some("10"));
}

// Assignment ----------------------------------------------------------------------------------------------------------

#[test]
fn eval_assign_simple() {
    let mut env = Environment::new();
    let expr = assign("x", ArithAssignOp::Assign, num(42));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 42);
    assert_eq!(env.get_var("x"), Some("42"));
}

#[test]
fn eval_assign_add() {
    let mut env = Environment::new();
    env.set_var("x", "10").unwrap();
    let expr = assign("x", ArithAssignOp::AddAssign, num(5));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 15);
    assert_eq!(env.get_var("x"), Some("15"));
}

#[test]
fn eval_assign_sub() {
    let mut env = Environment::new();
    env.set_var("x", "10").unwrap();
    let expr = assign("x", ArithAssignOp::SubAssign, num(3));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 7);
    assert_eq!(env.get_var("x"), Some("7"));
}

#[test]
fn eval_assign_mul() {
    let mut env = Environment::new();
    env.set_var("x", "6").unwrap();
    let expr = assign("x", ArithAssignOp::MulAssign, num(7));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 42);
    assert_eq!(env.get_var("x"), Some("42"));
}

#[test]
fn eval_assign_div() {
    let mut env = Environment::new();
    env.set_var("x", "15").unwrap();
    let expr = assign("x", ArithAssignOp::DivAssign, num(4));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 3);
    assert_eq!(env.get_var("x"), Some("3"));
}

#[test]
fn eval_assign_div_by_zero() {
    let mut env = Environment::new();
    env.set_var("x", "10").unwrap();
    let expr = assign("x", ArithAssignOp::DivAssign, num(0));
    let err = evaluate_arith_expr(&expr, &mut env).unwrap_err();
    assert!(matches!(err, ExecError::DivisionByZero));
}

#[test]
fn eval_assign_readonly_error() {
    let mut env = Environment::new();
    env.set_var("x", "10").unwrap();
    env.set_readonly("x");
    let expr = assign("x", ArithAssignOp::Assign, num(5));
    let err = evaluate_arith_expr(&expr, &mut env).unwrap_err();
    assert!(matches!(err, ExecError::ReadonlyVariable(_)));
}

// Group ---------------------------------------------------------------------------------------------------------------

#[test]
fn eval_group() {
    let mut env = Environment::new();
    let expr = group(binary(num(2), ArithBinaryOp::Add, num(3)));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 5);
}

// Comma ---------------------------------------------------------------------------------------------------------------

#[test]
fn eval_comma_returns_right() {
    let mut env = Environment::new();
    let expr = comma(num(1), num(2));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 2);
}

#[test]
fn eval_comma_evaluates_left_side_effect() {
    let mut env = Environment::new();
    let expr = comma(assign("x", ArithAssignOp::Assign, num(42)), num(0));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 0);
    assert_eq!(env.get_var("x"), Some("42"));
}

// Variable with expression --------------------------------------------------------------------------------------------

#[test]
fn eval_variable_in_expression() {
    let mut env = Environment::new();
    env.set_var("a", "10").unwrap();
    env.set_var("b", "20").unwrap();
    let expr = binary(var("a"), ArithBinaryOp::Add, var("b"));
    assert_eq!(evaluate_arith_expr(&expr, &mut env).unwrap(), 30);
}

// parse_i64 edge cases ------------------------------------------------------------------------------------------------

#[test]
fn eval_variable_hex() {
    let mut env = Environment::new();
    env.set_var("x", "0xFF").unwrap();
    assert_eq!(evaluate_arith_expr(&var("x"), &mut env).unwrap(), 255);
}

#[test]
fn eval_variable_octal() {
    let mut env = Environment::new();
    env.set_var("x", "010").unwrap();
    assert_eq!(evaluate_arith_expr(&var("x"), &mut env).unwrap(), 8);
}
