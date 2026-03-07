//! POSIX `test`/`[` builtin evaluator.
//!
//! Uses POSIX arg-count rules for 0-4 arguments and recursive descent for 5+.
//! Reuses `bash_test::evaluate_unary` and `bash_test::evaluate_binary` for
//! operator evaluation to avoid duplicating file/string/integer logic.

use crate::ast::{BinaryTestOp, UnaryTestOp};
use crate::exec::Environment;

/// Evaluate a POSIX `test` expression. Returns `Ok(true/false)` on success,
/// `Err(message)` on syntax error (which the caller maps to exit code 2).
pub(crate) fn run(args: &[String], env: &mut Environment) -> Result<bool, String> {
    match args.len() {
        0 => Ok(false),
        1 => Ok(!args[0].is_empty()),
        2 => eval_two(args, env),
        3 => eval_three(args, env),
        4 => eval_four(args, env),
        _ => eval_recursive(args, env),
    }
}

// POSIX arg-count rules ===============================================================================================

/// 2 args: `! EXPR` | `UNARY_OP WORD` | bare non-empty string test on first arg.
fn eval_two(args: &[String], env: &mut Environment) -> Result<bool, String> {
    if args[0] == "!" {
        return Ok(args[1].is_empty());
    }
    if let Some(op) = str_to_unary_op(&args[0]) {
        return Ok(eval_unary(op, &args[1], env));
    }
    // Unknown unary: POSIX says unspecified. Bash treats as error for some
    // operator-like strings and as non-empty-string test for others.
    // We follow bash: treat first arg as bare string (non-empty = true),
    // second is extra -> syntax error.
    Err(format!("test: unexpected argument '{}'", args[1]))
}

/// 3 args: `WORD BINARY_OP WORD` | `! TWO_ARG` | `( EXPR )` | `WORD -a WORD` | `WORD -o WORD`.
fn eval_three(args: &[String], env: &mut Environment) -> Result<bool, String> {
    // Binary test primary (=, !=, -eq, etc.)
    if is_binary_test_op(&args[1]) {
        return eval_binary(&args[0], &args[1], &args[2], env);
    }
    // Logical AND/OR
    if args[1] == "-a" {
        let l = run(&args[0..1], env)?;
        let r = run(&args[2..3], env)?;
        return Ok(l && r);
    }
    if args[1] == "-o" {
        let l = run(&args[0..1], env)?;
        let r = run(&args[2..3], env)?;
        return Ok(l || r);
    }
    // Negation of 2-arg
    if args[0] == "!" {
        return eval_two(&args[1..], env).map(|b| !b);
    }
    // Parenthesized 1-arg
    if args[0] == "(" && args[2] == ")" {
        return run(&args[1..2], env);
    }
    Err(format!("test: unknown binary operator '{}'", args[1]))
}

/// 4 args: `! THREE_ARG` | `( TWO_ARG )`.
fn eval_four(args: &[String], env: &mut Environment) -> Result<bool, String> {
    if args[0] == "!" {
        return eval_three(&args[1..], env).map(|b| !b);
    }
    if args[0] == "(" && args[3] == ")" {
        return eval_two(&args[1..3], env);
    }
    // Try recursive descent as fallback for complex expressions
    eval_recursive(args, env)
}

// Recursive descent for 5+ args =======================================================================================

fn eval_recursive(args: &[String], env: &mut Environment) -> Result<bool, String> {
    let mut eval = TestEvaluator { args, pos: 0, env };
    let result = eval.parse_or()?;
    if eval.pos < eval.args.len() {
        return Err(format!("test: unexpected argument '{}'", eval.args[eval.pos]));
    }
    Ok(result)
}

struct TestEvaluator<'a> {
    args: &'a [String],
    pos: usize,
    env: &'a mut Environment,
}

impl<'a> TestEvaluator<'a> {
    fn peek(&self) -> Option<&str> {
        self.args.get(self.pos).map(|s| s.as_str())
    }

    fn peek_at(&self, offset: usize) -> Option<&str> {
        self.args.get(self.pos + offset).map(|s| s.as_str())
    }

    fn advance(&mut self) -> &str {
        let s = &self.args[self.pos];
        self.pos += 1;
        s
    }

    fn remaining(&self) -> usize {
        self.args.len() - self.pos
    }

    /// `or_expr -> and_expr ("-o" and_expr)*`
    fn parse_or(&mut self) -> Result<bool, String> {
        let mut result = self.parse_and()?;
        while self.peek() == Some("-o") {
            self.advance();
            let rhs = self.parse_and()?;
            result = result || rhs;
        }
        Ok(result)
    }

    /// `and_expr -> not_expr ("-a" not_expr)*`
    fn parse_and(&mut self) -> Result<bool, String> {
        let mut result = self.parse_not()?;
        while self.peek() == Some("-a") {
            self.advance();
            let rhs = self.parse_not()?;
            result = result && rhs;
        }
        Ok(result)
    }

    /// `not_expr -> "!" not_expr | primary`
    fn parse_not(&mut self) -> Result<bool, String> {
        if self.peek() == Some("!") {
            self.advance();
            Ok(!self.parse_not()?)
        } else {
            self.parse_primary()
        }
    }

    /// ```text
    /// primary -> "(" expr ")"
    ///          | UNARY_OP WORD
    ///          | WORD BINARY_OP WORD
    ///          | WORD
    /// ```
    fn parse_primary(&mut self) -> Result<bool, String> {
        let token = self.peek().ok_or_else(|| "test: expected expression".to_string())?;

        // Parenthesized group
        if token == "(" {
            self.advance();
            let result = self.parse_or()?;
            if self.peek() != Some(")") {
                return Err("test: missing ')'".to_string());
            }
            self.advance();
            return Ok(result);
        }

        // Binary test primary: WORD OP WORD — check before unary so that
        // `test -z = -z` is parsed as string comparison
        if self.remaining() >= 3 {
            if let Some(next) = self.peek_at(1) {
                if is_binary_test_op(next) {
                    let left = self.advance().to_string();
                    let op_str = self.advance().to_string();
                    let right = self.advance().to_string();
                    return eval_binary(&left, &op_str, &right, self.env);
                }
            }
        }

        // Unary operator: OP WORD
        if self.remaining() >= 2 {
            if let Some(op) = str_to_unary_op(token) {
                self.advance();
                let operand = self.advance().to_string();
                return Ok(eval_unary(op, &operand, self.env));
            }
        }

        // Bare string: true if non-empty
        let s = self.advance();
        Ok(!s.is_empty())
    }
}

// Shared evaluation ===================================================================================================

fn eval_unary(op: UnaryTestOp, operand: &str, env: &Environment) -> bool {
    super::bash_test::evaluate_unary(op, operand, env)
}

fn eval_binary(left: &str, op: &str, right: &str, env: &mut Environment) -> Result<bool, String> {
    match op {
        // String comparisons: literal (not glob like [[ ]])
        "=" | "==" => Ok(left == right),
        "!=" => Ok(left != right),
        _ => {
            if let Some(bop) = str_to_binary_op(op) {
                super::bash_test::evaluate_binary(left, bop, right, env).map_err(|e| format!("test: {e}"))
            } else {
                Err(format!("test: unknown binary operator '{op}'"))
            }
        }
    }
}

/// Returns true for binary test primaries (not `-a`/`-o` logical connectives).
fn is_binary_test_op(s: &str) -> bool {
    matches!(
        s,
        "=" | "==" | "!=" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | "-nt" | "-ot" | "-ef"
    )
}

// Operator mapping ====================================================================================================

fn str_to_unary_op(s: &str) -> Option<UnaryTestOp> {
    match s {
        "-a" | "-e" => Some(UnaryTestOp::FileExists),
        "-b" => Some(UnaryTestOp::FileIsBlockDev),
        "-c" => Some(UnaryTestOp::FileIsCharDev),
        "-d" => Some(UnaryTestOp::FileIsDirectory),
        "-f" => Some(UnaryTestOp::FileIsRegular),
        "-g" => Some(UnaryTestOp::FileIsSetgid),
        "-G" => Some(UnaryTestOp::FileIsOwnedByGroup),
        "-h" | "-L" => Some(UnaryTestOp::FileIsSymlink),
        "-k" => Some(UnaryTestOp::FileIsSticky),
        "-n" => Some(UnaryTestOp::StringIsNonEmpty),
        "-N" => Some(UnaryTestOp::FileModifiedSinceRead),
        "-O" => Some(UnaryTestOp::FileIsOwnedByUser),
        "-p" => Some(UnaryTestOp::FileIsPipe),
        "-r" => Some(UnaryTestOp::FileIsReadable),
        "-s" => Some(UnaryTestOp::FileHasSize),
        "-S" => Some(UnaryTestOp::FileIsSocket),
        "-t" => Some(UnaryTestOp::FileDescriptorOpen),
        "-u" => Some(UnaryTestOp::FileIsSetuid),
        "-v" => Some(UnaryTestOp::VariableIsSet),
        "-w" => Some(UnaryTestOp::FileIsWritable),
        "-x" => Some(UnaryTestOp::FileIsExecutable),
        "-z" => Some(UnaryTestOp::StringIsEmpty),
        _ => None,
    }
}

fn str_to_binary_op(s: &str) -> Option<BinaryTestOp> {
    match s {
        "-eq" => Some(BinaryTestOp::IntEq),
        "-ne" => Some(BinaryTestOp::IntNe),
        "-lt" => Some(BinaryTestOp::IntLt),
        "-le" => Some(BinaryTestOp::IntLe),
        "-gt" => Some(BinaryTestOp::IntGt),
        "-ge" => Some(BinaryTestOp::IntGe),
        "-nt" => Some(BinaryTestOp::FileNewerThan),
        "-ot" => Some(BinaryTestOp::FileOlderThan),
        "-ef" => Some(BinaryTestOp::FileSameDevice),
        _ => None,
    }
}
