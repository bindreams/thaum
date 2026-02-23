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

// Arithmetic parser ===================================================================================================

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
            _ => Err(format!("unexpected token {:?} in arithmetic expression", self.peek())),
        }
    }
}

// Public API ==========================================================================================================

/// Parse a bash arithmetic expression from a raw string.
///
/// Called from:
/// - `parse_subshell_or_arithmetic` in `compound.rs` for `(( expr ))`
/// - The word parser in `word/mod.rs` for `$(( expr ))`
pub(crate) fn parse_arith_expr(input: &str) -> Result<ArithExpr, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        // Bash treats (( )) and $(( )) as evaluating to 0.
        return Ok(ArithExpr::Number(0));
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

// Tests ===============================================================================================================

#[cfg(test)]
#[path = "arith_expr/tests.rs"]
mod tests;
