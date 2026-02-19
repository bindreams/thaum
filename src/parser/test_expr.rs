use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::helpers::make_word;
use super::Parser;

impl<'src> Parser<'src> {
    /// Parse the boolean expression inside `[[ ... ]]`.
    ///
    /// Entry point — delegates to `parse_test_or` which is the lowest-precedence
    /// level. Precedence (low to high): `||`, `&&`, `!`, primary.
    pub(super) fn parse_test_expression(&mut self) -> Result<BashTestExpr, ParseError> {
        self.parse_test_or()
    }

    /// `parse_test_or → parse_test_and ( || parse_test_and )*`
    fn parse_test_or(&mut self) -> Result<BashTestExpr, ParseError> {
        let mut left = self.parse_test_and()?;
        while self.stream.peek()?.token == Token::OrIf {
            self.stream.advance()?;
            let right = self.parse_test_and()?;
            left = BashTestExpr::Or {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// `parse_test_and → parse_test_not ( && parse_test_not )*`
    fn parse_test_and(&mut self) -> Result<BashTestExpr, ParseError> {
        let mut left = self.parse_test_not()?;
        while self.stream.peek()?.token == Token::AndIf {
            self.stream.advance()?;
            let right = self.parse_test_not()?;
            left = BashTestExpr::And {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// `parse_test_not → ! parse_test_not | parse_test_primary`
    fn parse_test_not(&mut self) -> Result<BashTestExpr, ParseError> {
        if let Token::Word(w) = &self.stream.peek()?.token {
            if w == "!" {
                self.stream.advance()?;
                let inner = self.parse_test_not()?;
                return Ok(BashTestExpr::Not(Box::new(inner)));
            }
        }
        self.parse_test_primary()
    }

    /// Parse a primary test expression:
    /// - `( expr )` — grouped sub-expression
    /// - `-op word` — unary test
    /// - `word op word` — binary test (needs lookahead)
    /// - `word` — bare word (implicit `-n`)
    fn parse_test_primary(&mut self) -> Result<BashTestExpr, ParseError> {
        let peeked = self.stream.peek()?.clone();

        // Grouped expression: ( expr )
        // Inside [[ ]], '(' and ')' may arrive as Token::LParen/RParen or as
        // Token::Word("(")/Word(")") depending on context.
        let is_lparen = matches!(&peeked.token, Token::LParen)
            || matches!(&peeked.token, Token::Word(w) if w == "(");
        if is_lparen {
            self.stream.advance()?;
            let inner = self.parse_test_or()?;
            // Expect closing )
            let close = self.stream.peek()?;
            let is_rparen = matches!(&close.token, Token::RParen)
                || matches!(&close.token, Token::Word(w) if w == ")");
            if is_rparen {
                self.stream.advance()?;
            } else {
                return Err(ParseError::UnexpectedToken {
                    found: close.token.display_name().to_string(),
                    expected: "')' to close grouped test expression".to_string(),
                    span: close.span,
                });
            }
            return Ok(BashTestExpr::Group(Box::new(inner)));
        }

        // Unary test: -op word
        if let Token::Word(w) = &peeked.token {
            if let Some(op) = Self::parse_unary_test_op(w) {
                self.stream.advance()?;
                let arg_word = self.consume_test_word()?;
                return Ok(BashTestExpr::Unary { op, arg: arg_word });
            }
        }

        // Consume the first word, then check for binary operator
        let first_word = self.consume_test_word()?;

        // Check if next token is a binary operator
        if let Some(op) = self.peek_binary_test_op()? {
            self.advance_binary_op()?;
            let right_word = if op == BinaryTestOp::RegexMatch {
                // After `=~`, the RHS is a regex pattern where unquoted `(`, `)`,
                // `|` are metacharacters, not shell syntax.
                self.consume_regex_pattern()?
            } else {
                self.consume_test_word()?
            };
            return Ok(BashTestExpr::Binary {
                left: first_word,
                op,
                right: right_word,
            });
        }

        // Bare word — implicit -n test
        Ok(BashTestExpr::Word(first_word))
    }

    /// Consume all tokens up to `]]` as a regex pattern literal.
    ///
    /// After `=~`, shell operators like `(`, `)`, `|` are regex metacharacters.
    /// We collect them as raw text into a single `Fragment::Literal` word.
    fn consume_regex_pattern(&mut self) -> Result<Word, ParseError> {
        let start_span = self.stream.peek()?.span;
        let mut text = String::new();
        let mut end_span = start_span;

        loop {
            let peeked = self.stream.peek()?.clone();
            match &peeked.token {
                Token::BashDblRBracket | Token::Eof | Token::Newline => break,
                Token::Word(s) => {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(s);
                    end_span = peeked.span;
                    self.stream.advance()?;
                }
                _ => {
                    // Operators become literal regex text
                    let ch = match &peeked.token {
                        Token::LParen => "(",
                        Token::RParen => ")",
                        Token::Pipe => "|",
                        Token::Ampersand => "&",
                        Token::Semicolon => ";",
                        Token::AndIf => "&&",
                        Token::OrIf => "||",
                        Token::RedirectFromFile => "<",
                        Token::RedirectToFile => ">",
                        other => other.display_name().trim_matches('\''),
                    };
                    text.push_str(ch);
                    end_span = peeked.span;
                    self.stream.advance()?;
                }
            }
        }

        if text.is_empty() {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a regex pattern after =~".to_string(),
                span: start_span,
            });
        }

        Ok(Word {
            parts: vec![Fragment::Literal(text)],
            span: start_span.merge(end_span),
        })
    }

    /// Consume the next token as a word inside `[[ ]]`.
    ///
    /// Inside test expressions, most tokens are valid as words — operators like
    /// `<`, `>` can appear as arguments to binary tests, and special characters
    /// may be part of filenames or patterns.
    fn consume_test_word(&mut self) -> Result<Word, ParseError> {
        let peeked = self.stream.peek()?.clone();
        match &peeked.token {
            Token::Word(s) => {
                let word = make_word(s.clone(), peeked.span, &self.options);
                self.stream.advance()?;
                Ok(word)
            }
            Token::BashDblRBracket | Token::Eof => Err(ParseError::UnexpectedToken {
                found: peeked.token.display_name().to_string(),
                expected: "a word in test expression".to_string(),
                span: peeked.span,
            }),
            _ => {
                // Operators that are used as words inside [[ ]] (e.g., after
                // consuming a binary op the right-hand side might start with
                // something unexpected). Produce an error for non-word tokens
                // that aren't valid here.
                Err(ParseError::UnexpectedToken {
                    found: peeked.token.display_name().to_string(),
                    expected: "a word in test expression".to_string(),
                    span: peeked.span,
                })
            }
        }
    }

    /// Try to parse a known unary test operator from a word string.
    fn parse_unary_test_op(s: &str) -> Option<UnaryTestOp> {
        match s {
            "-e" => Some(UnaryTestOp::FileExists),
            "-f" => Some(UnaryTestOp::FileIsRegular),
            "-d" => Some(UnaryTestOp::FileIsDirectory),
            "-L" | "-h" => Some(UnaryTestOp::FileIsSymlink),
            "-b" => Some(UnaryTestOp::FileIsBlockDev),
            "-c" => Some(UnaryTestOp::FileIsCharDev),
            "-p" => Some(UnaryTestOp::FileIsPipe),
            "-S" => Some(UnaryTestOp::FileIsSocket),
            "-s" => Some(UnaryTestOp::FileHasSize),
            "-t" => Some(UnaryTestOp::FileDescriptorOpen),
            "-r" => Some(UnaryTestOp::FileIsReadable),
            "-w" => Some(UnaryTestOp::FileIsWritable),
            "-x" => Some(UnaryTestOp::FileIsExecutable),
            "-u" => Some(UnaryTestOp::FileIsSetuid),
            "-g" => Some(UnaryTestOp::FileIsSetgid),
            "-k" => Some(UnaryTestOp::FileIsSticky),
            "-O" => Some(UnaryTestOp::FileIsOwnedByUser),
            "-G" => Some(UnaryTestOp::FileIsOwnedByGroup),
            "-N" => Some(UnaryTestOp::FileModifiedSinceRead),
            "-z" => Some(UnaryTestOp::StringIsEmpty),
            "-n" => Some(UnaryTestOp::StringIsNonEmpty),
            "-v" => Some(UnaryTestOp::VariableIsSet),
            "-R" => Some(UnaryTestOp::VariableIsNameRef),
            _ => None,
        }
    }

    /// Peek at the next token to check if it is a binary test operator.
    /// Returns the operator kind if recognized, `None` otherwise.
    fn peek_binary_test_op(&mut self) -> Result<Option<BinaryTestOp>, ParseError> {
        let peeked = self.stream.peek()?;
        match &peeked.token {
            Token::Word(s) => Ok(Self::word_as_binary_test_op(s)),
            Token::RedirectFromFile => Ok(Some(BinaryTestOp::StringLessThan)),
            Token::RedirectToFile => Ok(Some(BinaryTestOp::StringGreaterThan)),
            _ => Ok(None),
        }
    }

    /// Try to classify a word string as a binary test operator.
    fn word_as_binary_test_op(s: &str) -> Option<BinaryTestOp> {
        match s {
            "==" | "=" => Some(BinaryTestOp::StringEquals),
            "!=" => Some(BinaryTestOp::StringNotEquals),
            "=~" => Some(BinaryTestOp::RegexMatch),
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

    /// Advance past a binary operator token. Called after `peek_binary_test_op`
    /// returned `Some`.
    fn advance_binary_op(&mut self) -> Result<(), ParseError> {
        self.stream.advance()?;
        Ok(())
    }
}
