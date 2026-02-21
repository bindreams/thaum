use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::Parser;

impl Parser {
    pub(super) fn parse_test_expression(&mut self) -> Result<BashTestExpr, ParseError> {
        self.parse_test_or()
    }

    fn parse_test_or(&mut self) -> Result<BashTestExpr, ParseError> {
        let mut left = self.parse_test_and()?;
        self.skip_linebreak()?;
        while self.lexer.peek()?.token == Token::OrIf {
            self.lexer.advance()?;
            let right = self.parse_test_and()?;
            left = BashTestExpr::Or {
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_linebreak()?;
        }
        Ok(left)
    }

    fn parse_test_and(&mut self) -> Result<BashTestExpr, ParseError> {
        let mut left = self.parse_test_not()?;
        self.skip_linebreak()?;
        while self.lexer.peek()?.token == Token::AndIf {
            self.lexer.advance()?;
            let right = self.parse_test_not()?;
            left = BashTestExpr::And {
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_linebreak()?;
        }
        Ok(left)
    }

    fn parse_test_not(&mut self) -> Result<BashTestExpr, ParseError> {
        self.skip_linebreak()?;
        let tok = self.lexer.peek()?.token.clone();
        if tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "!") {
            self.lexer.advance()?;
            let inner = self.parse_test_not()?;
            return Ok(BashTestExpr::Not(Box::new(inner)));
        }
        self.parse_test_primary()
    }

    fn parse_test_primary(&mut self) -> Result<BashTestExpr, ParseError> {
        self.skip_linebreak()?;
        let peeked = self.lexer.peek()?.clone();

        // Grouped expression: ( expr )
        let is_lparen = matches!(&peeked.token, Token::LParen)
            || matches!(&peeked.token, Token::Literal(w) if w == "(");
        if is_lparen {
            self.lexer.advance()?;
            let saved = self.in_test_group;
            self.in_test_group = true;
            let inner = self.parse_test_or()?;
            self.in_test_group = saved;
            let close = self.lexer.peek()?;
            let is_rparen = matches!(&close.token, Token::RParen)
                || matches!(&close.token, Token::Literal(w) if w == ")");
            if is_rparen {
                self.lexer.advance()?;
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
        if let Token::Literal(w) = &peeked.token {
            if let Some(op) = Self::parse_unary_test_op(w) {
                self.lexer.advance()?;
                let arg_word = self.consume_test_word()?;
                return Ok(BashTestExpr::Unary { op, arg: arg_word });
            }
        }

        // Consume the first word, then check for binary operator
        let first_word = self.consume_test_word()?;

        self.lexer.eat_whitespace()?;
        if let Some(op) = self.lexer.peek()?.token.as_binary_test_op() {
            self.advance_binary_op()?;
            let right_word = if op == BinaryTestOp::RegexMatch {
                self.consume_regex_pattern(self.in_test_group)?
            } else {
                self.consume_test_word()?
            };
            return Ok(BashTestExpr::Binary {
                left: first_word,
                op,
                right: right_word,
            });
        }

        Ok(BashTestExpr::Word(first_word))
    }

    fn consume_regex_pattern(&mut self, in_group: bool) -> Result<Word, ParseError> {
        self.lexer.eat_whitespace()?;
        let start_span = self.lexer.peek()?.span;
        let mut text = String::new();
        let mut end_span = start_span;
        let mut paren_depth = 0i32;

        loop {
            let peeked = self.lexer.peek()?.clone();
            match &peeked.token {
                Token::BashDblRBracket | Token::Eof | Token::Newline => break,
                // ) at depth 0 inside a grouped expression (e.g., [[ (foo =~ bar) ]])
                // closes the group, not the regex. Outside a group, ) is regex content.
                Token::RParen if in_group && paren_depth <= 0 => break,
                tok if tok.is_fragment() => {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    match tok {
                        Token::Literal(s) => text.push_str(s),
                        Token::SingleQuoted(s) => text.push_str(s),
                        Token::DoubleQuoted(s) => text.push_str(s),
                        _ => text.push_str(tok.display_name().trim_matches('\'')),
                    }
                    end_span = peeked.span;
                    self.lexer.advance()?;
                }
                _ => {
                    match &peeked.token {
                        Token::LParen => paren_depth += 1,
                        Token::RParen => paren_depth -= 1,
                        _ => {}
                    }
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
                    self.lexer.advance()?;
                }
            }
        }

        if text.is_empty() {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a regex pattern after =~".to_string(),
                span: start_span,
            });
        }

        Ok(Word {
            parts: vec![Fragment::Literal(text)],
            span: start_span.merge(end_span),
        })
    }

    fn consume_test_word(&mut self) -> Result<Word, ParseError> {
        self.skip_linebreak()?;
        if self.lexer.peek()?.token.is_fragment() {
            return Ok(self.collect_word()?.unwrap());
        }

        let peeked = self.lexer.peek()?.clone();
        match &peeked.token {
            Token::BashDblRBracket | Token::Eof => Err(ParseError::UnexpectedToken {
                found: peeked.token.display_name().to_string(),
                expected: "a word in test expression".to_string(),
                span: peeked.span,
            }),
            _ => Err(ParseError::UnexpectedToken {
                found: peeked.token.display_name().to_string(),
                expected: "a word in test expression".to_string(),
                span: peeked.span,
            }),
        }
    }

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

    fn advance_binary_op(&mut self) -> Result<(), ParseError> {
        self.lexer.advance()?;
        Ok(())
    }
}
