use crate::ast::{ArithAssignOp, ArithBinaryOp, ArithExpr, ArithUnaryOp};
use crate::exec::environment::Environment;
use crate::exec::error::ExecError;

/// Evaluate an arithmetic expression, returning its integer value.
///
/// Arithmetic follows bash semantics:
/// - All values are i64 (signed 64-bit)
/// - Unset/empty variables evaluate to 0
/// - Non-numeric variables are an error
/// - Overflow wraps (C-style)
/// - Assignments and increment/decrement modify variables in `env`
pub fn evaluate_arith_expr(expr: &ArithExpr, env: &mut Environment) -> Result<i64, ExecError> {
    match expr {
        ArithExpr::Number(n) => Ok(*n),

        // TODO: Bash supports recursive variable expansion in arithmetic:
        // `a=b; b=5; echo $((a))` evaluates to 5. We currently treat
        // non-numeric variable values as errors.
        ArithExpr::Variable(name) => read_var_as_i64(name, env),

        ArithExpr::Binary { left, op, right } => eval_binary(left, *op, right, env),

        ArithExpr::UnaryPrefix { op, operand } => eval_unary_prefix(*op, operand, env),

        ArithExpr::UnaryPostfix { operand, op } => eval_unary_postfix(operand, *op, env),

        ArithExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let cond = evaluate_arith_expr(condition, env)?;
            if cond != 0 {
                evaluate_arith_expr(then_expr, env)
            } else {
                evaluate_arith_expr(else_expr, env)
            }
        }

        ArithExpr::Assignment { target, op, value } => eval_assignment(target, *op, value, env),

        ArithExpr::Group(inner) => evaluate_arith_expr(inner, env),

        ArithExpr::Comma { left, right } => {
            evaluate_arith_expr(left, env)?;
            evaluate_arith_expr(right, env)
        }
    }
}

/// Read a variable's value as i64. Unset or empty → 0.
fn read_var_as_i64(name: &str, env: &Environment) -> Result<i64, ExecError> {
    match env.get_var(name) {
        None | Some("") => Ok(0),
        Some(s) => parse_i64(name, s),
    }
}

/// Parse a string as i64, supporting decimal, hex (0x), and octal (0) prefixes.
fn parse_i64(context: &str, s: &str) -> Result<i64, ExecError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }

    // Handle optional leading sign
    let (negative, digits) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest.trim_start())
    } else if let Some(rest) = s.strip_prefix('+') {
        (false, rest.trim_start())
    } else {
        (false, s)
    };

    let abs = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16)
    } else if digits.starts_with('0')
        && digits.len() > 1
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        i64::from_str_radix(digits, 8)
    } else {
        digits.parse::<i64>()
    };

    match abs {
        Ok(v) => Ok(if negative { v.wrapping_neg() } else { v }),
        Err(_) => Err(ExecError::InvalidNumber(
            format!("{}: expression", context),
            s.to_string(),
        )),
    }
}

/// Evaluate a binary operation.
fn eval_binary(
    left: &ArithExpr,
    op: ArithBinaryOp,
    right: &ArithExpr,
    env: &mut Environment,
) -> Result<i64, ExecError> {
    // Short-circuit operators
    match op {
        ArithBinaryOp::LogAnd => {
            let l = evaluate_arith_expr(left, env)?;
            if l == 0 {
                return Ok(0);
            }
            let r = evaluate_arith_expr(right, env)?;
            return Ok(if r != 0 { 1 } else { 0 });
        }
        ArithBinaryOp::LogOr => {
            let l = evaluate_arith_expr(left, env)?;
            if l != 0 {
                return Ok(1);
            }
            let r = evaluate_arith_expr(right, env)?;
            return Ok(if r != 0 { 1 } else { 0 });
        }
        _ => {}
    }

    let l = evaluate_arith_expr(left, env)?;
    let r = evaluate_arith_expr(right, env)?;

    match op {
        ArithBinaryOp::Add => Ok(l.wrapping_add(r)),
        ArithBinaryOp::Sub => Ok(l.wrapping_sub(r)),
        ArithBinaryOp::Mul => Ok(l.wrapping_mul(r)),
        ArithBinaryOp::Div => {
            if r == 0 {
                return Err(ExecError::DivisionByZero);
            }
            Ok(l.wrapping_div(r))
        }
        ArithBinaryOp::Mod => {
            if r == 0 {
                return Err(ExecError::DivisionByZero);
            }
            Ok(l.wrapping_rem(r))
        }
        ArithBinaryOp::Exp => int_pow(l, r),
        ArithBinaryOp::ShiftLeft => Ok(l.wrapping_shl(r as u32)),
        ArithBinaryOp::ShiftRight => Ok(l.wrapping_shr(r as u32)),
        ArithBinaryOp::BitAnd => Ok(l & r),
        ArithBinaryOp::BitOr => Ok(l | r),
        ArithBinaryOp::BitXor => Ok(l ^ r),
        ArithBinaryOp::Eq => Ok(if l == r { 1 } else { 0 }),
        ArithBinaryOp::Ne => Ok(if l != r { 1 } else { 0 }),
        ArithBinaryOp::Lt => Ok(if l < r { 1 } else { 0 }),
        ArithBinaryOp::Le => Ok(if l <= r { 1 } else { 0 }),
        ArithBinaryOp::Gt => Ok(if l > r { 1 } else { 0 }),
        ArithBinaryOp::Ge => Ok(if l >= r { 1 } else { 0 }),
        // Already handled above
        ArithBinaryOp::LogAnd | ArithBinaryOp::LogOr => unreachable!(),
    }
}

/// Integer exponentiation with wrapping.
fn int_pow(base: i64, exp: i64) -> Result<i64, ExecError> {
    if exp < 0 {
        // Bash: negative exponent is an error
        return Err(ExecError::InvalidNumber(
            "exponent".to_string(),
            "exponent less than 0".to_string(),
        ));
    }

    let mut result: i64 = 1;
    let mut b = base;
    let mut e = exp as u64;
    while e > 0 {
        if e & 1 == 1 {
            result = result.wrapping_mul(b);
        }
        e >>= 1;
        if e > 0 {
            b = b.wrapping_mul(b);
        }
    }
    Ok(result)
}

/// Evaluate a unary prefix operation.
fn eval_unary_prefix(
    op: ArithUnaryOp,
    operand: &ArithExpr,
    env: &mut Environment,
) -> Result<i64, ExecError> {
    match op {
        ArithUnaryOp::Negate => {
            let v = evaluate_arith_expr(operand, env)?;
            Ok(v.wrapping_neg())
        }
        ArithUnaryOp::Plus => evaluate_arith_expr(operand, env),
        ArithUnaryOp::LogNot => {
            let v = evaluate_arith_expr(operand, env)?;
            Ok(if v == 0 { 1 } else { 0 })
        }
        ArithUnaryOp::BitNot => {
            let v = evaluate_arith_expr(operand, env)?;
            Ok(!v)
        }
        ArithUnaryOp::Increment => {
            let name = expect_variable(operand)?;
            let old = read_var_as_i64(name, env)?;
            let new = old.wrapping_add(1);
            env.set_var(name, &new.to_string())?;
            Ok(new)
        }
        ArithUnaryOp::Decrement => {
            let name = expect_variable(operand)?;
            let old = read_var_as_i64(name, env)?;
            let new = old.wrapping_sub(1);
            env.set_var(name, &new.to_string())?;
            Ok(new)
        }
    }
}

/// Evaluate a unary postfix operation.
fn eval_unary_postfix(
    operand: &ArithExpr,
    op: ArithUnaryOp,
    env: &mut Environment,
) -> Result<i64, ExecError> {
    let name = expect_variable(operand)?;
    let old = read_var_as_i64(name, env)?;

    let new = match op {
        ArithUnaryOp::Increment => old.wrapping_add(1),
        ArithUnaryOp::Decrement => old.wrapping_sub(1),
        _ => {
            debug_assert!(false, "postfix operator must be Increment or Decrement");
            return Err(ExecError::BadSubstitution(
                "invalid postfix operator".to_string(),
            ));
        }
    };

    env.set_var(name, &new.to_string())?;
    Ok(old) // postfix returns old value
}

/// Evaluate an assignment expression.
fn eval_assignment(
    target: &str,
    op: ArithAssignOp,
    value: &ArithExpr,
    env: &mut Environment,
) -> Result<i64, ExecError> {
    let rhs = evaluate_arith_expr(value, env)?;

    let result = match op {
        ArithAssignOp::Assign => rhs,
        _ => {
            let lhs = read_var_as_i64(target, env)?;
            match op {
                ArithAssignOp::Assign => unreachable!(),
                ArithAssignOp::AddAssign => lhs.wrapping_add(rhs),
                ArithAssignOp::SubAssign => lhs.wrapping_sub(rhs),
                ArithAssignOp::MulAssign => lhs.wrapping_mul(rhs),
                ArithAssignOp::DivAssign => {
                    if rhs == 0 {
                        return Err(ExecError::DivisionByZero);
                    }
                    lhs.wrapping_div(rhs)
                }
                ArithAssignOp::ModAssign => {
                    if rhs == 0 {
                        return Err(ExecError::DivisionByZero);
                    }
                    lhs.wrapping_rem(rhs)
                }
                ArithAssignOp::ShiftLeftAssign => lhs.wrapping_shl(rhs as u32),
                ArithAssignOp::ShiftRightAssign => lhs.wrapping_shr(rhs as u32),
                ArithAssignOp::BitAndAssign => lhs & rhs,
                ArithAssignOp::BitOrAssign => lhs | rhs,
                ArithAssignOp::BitXorAssign => lhs ^ rhs,
            }
        }
    };

    env.set_var(target, &result.to_string())?;
    Ok(result)
}

/// Extract the variable name from an expression node.
///
/// Increment/decrement operators require a variable target. The parser
/// guarantees this, but we handle the error gracefully in release builds.
fn expect_variable(expr: &ArithExpr) -> Result<&str, ExecError> {
    match expr {
        ArithExpr::Variable(name) => Ok(name),
        _ => {
            debug_assert!(
                false,
                "increment/decrement operand must be a Variable, got {:?}",
                expr
            );
            Err(ExecError::BadSubstitution(
                "operand requires a variable".to_string(),
            ))
        }
    }
}

#[cfg(test)]
#[path = "arithmetic_tests.rs"]
mod tests;
