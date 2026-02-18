//! Arithmetic expression parser for bash `(( ))` and `$(( ))`.
//!
//! This module provides a self-contained recursive descent parser that operates
//! on a raw string (already extracted by the compound parser or word parser),
//! not on the shell token stream. It has its own mini-tokenizer.
//!
//! Operator precedence follows C (14 levels, low to high):
//! 1.  `,`  (comma)
//! 2.  `=`, `+=`, `-=`, etc. (assignment, right-associative)
//! 3.  `? :` (ternary, right-associative)
//! 4.  `||` (logical OR)
//! 5.  `&&` (logical AND)
//! 6.  `|`  (bitwise OR)
//! 7.  `^`  (bitwise XOR)
//! 8.  `&`  (bitwise AND)
//! 9.  `==`, `!=` (equality)
//! 10. `<`, `<=`, `>`, `>=` (comparison)
//! 11. `<<`, `>>` (shift)
//! 12. `+`, `-` (addition)
//! 13. `*`, `/`, `%` (multiplication)
//! 14. `**` (exponentiation, right-associative)
//! 15. Unary prefix: `+`, `-`, `!`, `~`, `++`, `--`
//! 16. Postfix: `++`, `--`
//! 17. Primary: number, variable, `( expr )`

mod lexer;

use crate::ast::{ArithAssignOp, ArithBinaryOp, ArithExpr, ArithUnaryOp};
use lexer::{ArithLexer, ArithToken};

// ============================================================================
// Arithmetic parser
// ============================================================================

struct ArithParser {
    lexer: ArithLexer,
    current: ArithToken,
}

impl ArithParser {
    fn new(input: &str) -> Result<Self, String> {
        let mut lexer = ArithLexer::new(input);
        let current = lexer.next_token()?;
        Ok(ArithParser { lexer, current })
    }

    fn advance(&mut self) -> Result<ArithToken, String> {
        let old = std::mem::replace(&mut self.current, ArithToken::Eof);
        self.current = self.lexer.next_token()?;
        Ok(old)
    }

    fn peek(&self) -> &ArithToken {
        &self.current
    }

    fn eat(&mut self, expected: &ArithToken) -> Result<bool, String> {
        if self.current == *expected {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect(&mut self, expected: &ArithToken) -> Result<(), String> {
        if self.current == *expected {
            self.advance()?;
            Ok(())
        } else {
            Err(format!("expected {:?}, found {:?}", expected, self.current))
        }
    }

    /// Parse a complete arithmetic expression.
    fn parse_expr(&mut self) -> Result<ArithExpr, String> {
        self.parse_comma()
    }

    // Level 1: Comma (left-associative)
    fn parse_comma(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_assignment()?;
        while *self.peek() == ArithToken::Comma {
            self.advance()?;
            let right = self.parse_assignment()?;
            left = ArithExpr::Comma {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 2: Assignment (right-associative)
    // Assignment target must be a variable name (or Ident).
    fn parse_assignment(&mut self) -> Result<ArithExpr, String> {
        let expr = self.parse_ternary()?;

        // Check if we have an assignment operator
        if let Some(op) = self.peek_assign_op() {
            // Extract the target variable name
            let target = match &expr {
                ArithExpr::Variable(name) => name.clone(),
                _ => return Err("assignment target must be a variable".to_string()),
            };
            self.advance()?; // consume the assignment operator
            let value = self.parse_assignment()?; // right-associative
            Ok(ArithExpr::Assignment {
                target,
                op,
                value: Box::new(value),
            })
        } else {
            Ok(expr)
        }
    }

    fn peek_assign_op(&self) -> Option<ArithAssignOp> {
        match self.peek() {
            ArithToken::Eq => Some(ArithAssignOp::Assign),
            ArithToken::PlusEq => Some(ArithAssignOp::AddAssign),
            ArithToken::MinusEq => Some(ArithAssignOp::SubAssign),
            ArithToken::StarEq => Some(ArithAssignOp::MulAssign),
            ArithToken::SlashEq => Some(ArithAssignOp::DivAssign),
            ArithToken::PercentEq => Some(ArithAssignOp::ModAssign),
            ArithToken::ShiftLeftEq => Some(ArithAssignOp::ShiftLeftAssign),
            ArithToken::ShiftRightEq => Some(ArithAssignOp::ShiftRightAssign),
            ArithToken::AmpEq => Some(ArithAssignOp::BitAndAssign),
            ArithToken::PipeEq => Some(ArithAssignOp::BitOrAssign),
            ArithToken::CaretEq => Some(ArithAssignOp::BitXorAssign),
            _ => None,
        }
    }

    // Level 3: Ternary (right-associative)
    fn parse_ternary(&mut self) -> Result<ArithExpr, String> {
        let condition = self.parse_logical_or()?;
        if self.eat(&ArithToken::Question)? {
            let then_expr = self.parse_assignment()?; // ternary branches allow assignment
            self.expect(&ArithToken::Colon)?;
            let else_expr = self.parse_ternary()?; // right-associative
            Ok(ArithExpr::Ternary {
                condition: Box::new(condition),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            })
        } else {
            Ok(condition)
        }
    }

    // Level 4: Logical OR (left-associative)
    fn parse_logical_or(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_logical_and()?;
        while *self.peek() == ArithToken::PipePipe {
            self.advance()?;
            let right = self.parse_logical_and()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op: ArithBinaryOp::LogOr,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 5: Logical AND (left-associative)
    fn parse_logical_and(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_bitwise_or()?;
        while *self.peek() == ArithToken::AmpAmp {
            self.advance()?;
            let right = self.parse_bitwise_or()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op: ArithBinaryOp::LogAnd,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 6: Bitwise OR (left-associative)
    fn parse_bitwise_or(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_bitwise_xor()?;
        while *self.peek() == ArithToken::Pipe {
            self.advance()?;
            let right = self.parse_bitwise_xor()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op: ArithBinaryOp::BitOr,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 7: Bitwise XOR (left-associative)
    fn parse_bitwise_xor(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_bitwise_and()?;
        while *self.peek() == ArithToken::Caret {
            self.advance()?;
            let right = self.parse_bitwise_and()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op: ArithBinaryOp::BitXor,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 8: Bitwise AND (left-associative)
    fn parse_bitwise_and(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_equality()?;
        while *self.peek() == ArithToken::Amp {
            self.advance()?;
            let right = self.parse_equality()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op: ArithBinaryOp::BitAnd,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 9: Equality (left-associative)
    fn parse_equality(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                ArithToken::EqEq => ArithBinaryOp::Eq,
                ArithToken::BangEq => ArithBinaryOp::Ne,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_comparison()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 10: Comparison (left-associative)
    fn parse_comparison(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_shift()?;
        loop {
            let op = match self.peek() {
                ArithToken::Lt => ArithBinaryOp::Lt,
                ArithToken::Le => ArithBinaryOp::Le,
                ArithToken::Gt => ArithBinaryOp::Gt,
                ArithToken::Ge => ArithBinaryOp::Ge,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_shift()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 11: Shift (left-associative)
    fn parse_shift(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.peek() {
                ArithToken::ShiftLeft => ArithBinaryOp::ShiftLeft,
                ArithToken::ShiftRight => ArithBinaryOp::ShiftRight,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_addition()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 12: Addition (left-associative)
    fn parse_addition(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek() {
                ArithToken::Plus => ArithBinaryOp::Add,
                ArithToken::Minus => ArithBinaryOp::Sub,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_multiplication()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 13: Multiplication (left-associative)
    fn parse_multiplication(&mut self) -> Result<ArithExpr, String> {
        let mut left = self.parse_exponentiation()?;
        loop {
            let op = match self.peek() {
                ArithToken::Star => ArithBinaryOp::Mul,
                ArithToken::Slash => ArithBinaryOp::Div,
                ArithToken::Percent => ArithBinaryOp::Mod,
                _ => break,
            };
            self.advance()?;
            let right = self.parse_exponentiation()?;
            left = ArithExpr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Level 14: Exponentiation (right-associative)
    fn parse_exponentiation(&mut self) -> Result<ArithExpr, String> {
        let base = self.parse_unary_prefix()?;
        if *self.peek() == ArithToken::StarStar {
            self.advance()?;
            let exp = self.parse_exponentiation()?; // right-associative
            Ok(ArithExpr::Binary {
                left: Box::new(base),
                op: ArithBinaryOp::Exp,
                right: Box::new(exp),
            })
        } else {
            Ok(base)
        }
    }

    // Level 15: Unary prefix: +, -, !, ~, ++, --
    fn parse_unary_prefix(&mut self) -> Result<ArithExpr, String> {
        match self.peek().clone() {
            ArithToken::Plus => {
                self.advance()?;
                let operand = self.parse_unary_prefix()?;
                Ok(ArithExpr::UnaryPrefix {
                    op: ArithUnaryOp::Plus,
                    operand: Box::new(operand),
                })
            }
            ArithToken::Minus => {
                self.advance()?;
                let operand = self.parse_unary_prefix()?;
                Ok(ArithExpr::UnaryPrefix {
                    op: ArithUnaryOp::Negate,
                    operand: Box::new(operand),
                })
            }
            ArithToken::Bang => {
                self.advance()?;
                let operand = self.parse_unary_prefix()?;
                Ok(ArithExpr::UnaryPrefix {
                    op: ArithUnaryOp::LogNot,
                    operand: Box::new(operand),
                })
            }
            ArithToken::Tilde => {
                self.advance()?;
                let operand = self.parse_unary_prefix()?;
                Ok(ArithExpr::UnaryPrefix {
                    op: ArithUnaryOp::BitNot,
                    operand: Box::new(operand),
                })
            }
            ArithToken::PlusPlus => {
                self.advance()?;
                let operand = self.parse_unary_prefix()?;
                Ok(ArithExpr::UnaryPrefix {
                    op: ArithUnaryOp::Increment,
                    operand: Box::new(operand),
                })
            }
            ArithToken::MinusMinus => {
                self.advance()?;
                let operand = self.parse_unary_prefix()?;
                Ok(ArithExpr::UnaryPrefix {
                    op: ArithUnaryOp::Decrement,
                    operand: Box::new(operand),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    // Level 16: Postfix: ++, --
    fn parse_postfix(&mut self) -> Result<ArithExpr, String> {
        let operand = self.parse_primary()?;
        match self.peek() {
            ArithToken::PlusPlus => {
                self.advance()?;
                Ok(ArithExpr::UnaryPostfix {
                    operand: Box::new(operand),
                    op: ArithUnaryOp::Increment,
                })
            }
            ArithToken::MinusMinus => {
                self.advance()?;
                Ok(ArithExpr::UnaryPostfix {
                    operand: Box::new(operand),
                    op: ArithUnaryOp::Decrement,
                })
            }
            _ => Ok(operand),
        }
    }

    // Level 17: Primary: number, variable, ( expr )
    fn parse_primary(&mut self) -> Result<ArithExpr, String> {
        match self.peek().clone() {
            ArithToken::Number(n) => {
                self.advance()?;
                Ok(ArithExpr::Number(n))
            }
            ArithToken::Ident(name) => {
                self.advance()?;
                Ok(ArithExpr::Variable(name))
            }
            ArithToken::Dollar => {
                // $var inside arithmetic — consume $ and read the variable
                self.advance()?;
                match self.peek().clone() {
                    ArithToken::Ident(name) => {
                        self.advance()?;
                        Ok(ArithExpr::Variable(name))
                    }
                    _ => Err("expected variable name after '$'".to_string()),
                }
            }
            ArithToken::LParen => {
                self.advance()?;
                let expr = self.parse_expr()?;
                self.expect(&ArithToken::RParen)?;
                Ok(ArithExpr::Group(Box::new(expr)))
            }
            _ => Err(format!(
                "unexpected token {:?} in arithmetic expression",
                self.peek()
            )),
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Parse a bash arithmetic expression from a raw string.
///
/// Called from:
/// - `parse_subshell_or_arithmetic` in `compound.rs` for `(( expr ))`
/// - The word parser in `word/mod.rs` for `$(( expr ))`
pub(crate) fn parse_arith_expr(input: &str) -> Result<ArithExpr, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty arithmetic expression".to_string());
    }
    let mut parser = ArithParser::new(trimmed)?;
    let expr = parser.parse_expr()?;
    if *parser.peek() != ArithToken::Eof {
        return Err(format!(
            "unexpected token {:?} after arithmetic expression",
            parser.peek()
        ));
    }
    Ok(expr)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(input: &str) -> ArithExpr {
        parse_arith_expr(input)
            .unwrap_or_else(|e| panic!("parse_arith_expr failed for {:?}: {}", input, e))
    }

    // --- Tokenizer tests ---

    #[test]
    fn lex_decimal_number() {
        assert_eq!(parse_ok("42"), ArithExpr::Number(42));
    }

    #[test]
    fn lex_hex_number() {
        assert_eq!(parse_ok("0x1F"), ArithExpr::Number(0x1F));
    }

    #[test]
    fn lex_hex_number_uppercase() {
        assert_eq!(parse_ok("0XFF"), ArithExpr::Number(0xFF));
    }

    #[test]
    fn lex_octal_number() {
        assert_eq!(parse_ok("077"), ArithExpr::Number(0o77));
    }

    #[test]
    fn lex_zero() {
        assert_eq!(parse_ok("0"), ArithExpr::Number(0));
    }

    // --- Variable tests ---

    #[test]
    fn bare_variable() {
        assert_eq!(parse_ok("x"), ArithExpr::Variable("x".to_string()));
    }

    #[test]
    fn dollar_variable() {
        assert_eq!(parse_ok("$x"), ArithExpr::Variable("x".to_string()));
    }

    #[test]
    fn variable_with_underscores() {
        assert_eq!(
            parse_ok("my_var"),
            ArithExpr::Variable("my_var".to_string())
        );
    }

    #[test]
    fn array_variable() {
        assert_eq!(
            parse_ok("arr[0]"),
            ArithExpr::Variable("arr[0]".to_string())
        );
    }

    // --- Binary operator tests ---

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    // --- Unary operator tests ---

    #[test]
    fn unary_negate() {
        assert_eq!(
            parse_ok("-x"),
            ArithExpr::UnaryPrefix {
                op: ArithUnaryOp::Negate,
                operand: Box::new(ArithExpr::Variable("x".to_string())),
            }
        );
    }

    #[test]
    fn unary_plus() {
        assert_eq!(
            parse_ok("+x"),
            ArithExpr::UnaryPrefix {
                op: ArithUnaryOp::Plus,
                operand: Box::new(ArithExpr::Variable("x".to_string())),
            }
        );
    }

    #[test]
    fn logical_not() {
        assert_eq!(
            parse_ok("!x"),
            ArithExpr::UnaryPrefix {
                op: ArithUnaryOp::LogNot,
                operand: Box::new(ArithExpr::Variable("x".to_string())),
            }
        );
    }

    #[test]
    fn bitwise_not() {
        assert_eq!(
            parse_ok("~x"),
            ArithExpr::UnaryPrefix {
                op: ArithUnaryOp::BitNot,
                operand: Box::new(ArithExpr::Variable("x".to_string())),
            }
        );
    }

    #[test]
    fn pre_increment() {
        assert_eq!(
            parse_ok("++x"),
            ArithExpr::UnaryPrefix {
                op: ArithUnaryOp::Increment,
                operand: Box::new(ArithExpr::Variable("x".to_string())),
            }
        );
    }

    #[test]
    fn pre_decrement() {
        assert_eq!(
            parse_ok("--x"),
            ArithExpr::UnaryPrefix {
                op: ArithUnaryOp::Decrement,
                operand: Box::new(ArithExpr::Variable("x".to_string())),
            }
        );
    }

    #[test]
    fn post_increment() {
        assert_eq!(
            parse_ok("x++"),
            ArithExpr::UnaryPostfix {
                operand: Box::new(ArithExpr::Variable("x".to_string())),
                op: ArithUnaryOp::Increment,
            }
        );
    }

    #[test]
    fn post_decrement() {
        assert_eq!(
            parse_ok("x--"),
            ArithExpr::UnaryPostfix {
                operand: Box::new(ArithExpr::Variable("x".to_string())),
                op: ArithUnaryOp::Decrement,
            }
        );
    }

    // --- Assignment tests ---

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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
                assert_eq!(*op, expected_op, "failed for {}", input);
            } else {
                panic!("expected Assignment for {}, got {:?}", input, expr);
            }
        }
    }

    #[test]
    fn assignment_right_associative() {
        // x = y = 5 → Assignment(x, Assign, Assignment(y, Assign, 5))
        let expr = parse_ok("x = y = 5");
        if let ArithExpr::Assignment { target, value, .. } = &expr {
            assert_eq!(target, "x");
            assert!(matches!(value.as_ref(), ArithExpr::Assignment { .. }));
        } else {
            panic!("expected Assignment, got {:?}", expr);
        }
    }

    // --- Ternary test ---

    #[test]
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

    // --- Grouping ---

    #[test]
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

    // --- Comma ---

    #[test]
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

    // --- Mixed expression ---

    #[test]
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

    #[test]
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
            panic!("expected Sub at top, got {:?}", expr);
        }
    }

    // --- Error cases ---

    #[test]
    fn error_empty_expression() {
        assert!(parse_arith_expr("").is_err());
    }

    #[test]
    fn error_only_whitespace() {
        assert!(parse_arith_expr("   ").is_err());
    }

    #[test]
    fn error_unclosed_paren() {
        assert!(parse_arith_expr("(1 + 2").is_err());
    }

    #[test]
    fn error_unexpected_token() {
        assert!(parse_arith_expr(")").is_err());
    }

    #[test]
    fn error_trailing_operator() {
        assert!(parse_arith_expr("1 +").is_err());
    }

    #[test]
    fn error_assignment_to_number() {
        assert!(parse_arith_expr("5 = 3").is_err());
    }
}
