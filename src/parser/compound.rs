use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::arith_expr::parse_arith_expr;
use super::helpers::*;
use super::Parser;

impl Parser {
    pub(super) fn parse_compound_command(&mut self) -> Result<CompoundCommand, ParseError> {
        self.lexer.skip_blanks()?;
        let tok = self.lexer.peek()?.token.clone();
        match &tok {
            Token::Literal(w) => match w.as_str() {
                "if" => self.parse_if_clause(),
                "while" => self.parse_while_clause(),
                "until" => self.parse_until_clause(),
                "for" => self.parse_for_clause(),
                "case" => self.parse_case_clause(),
                "{" => self.parse_brace_group(),
                "select" if self.options.select => self.parse_select_clause(),
                _ => Err(ParseError::UnexpectedToken {
                    found: self.lexer.peek()?.token.display_name().to_string(),
                    expected: "a compound command".to_string(),
                    span: self.lexer.peek()?.span,
                }),
            },
            Token::LParen => self.parse_subshell_or_arithmetic(),
            Token::BashDblLBracket => self.parse_double_bracket(),
            _ => Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a compound command".to_string(),
                span: self.lexer.peek()?.span,
            }),
        }
    }

    fn parse_if_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("if")?;

        let condition = self.parse_required_compound_list("if condition")?;
        self.expect_closing_keyword("then", "if", start_span)?;
        let then_body = self.parse_required_compound_list("then body")?;

        let mut elifs = Vec::new();
        while self.is_lone_literal("elif")? {
            let elif_span = self.lexer.peek()?.span;
            self.lexer.advance()?;
            let elif_cond = self.parse_required_compound_list("elif condition")?;
            self.expect_closing_keyword("then", "elif", elif_span)?;
            let elif_body = self.parse_required_compound_list("elif body")?;
            let end = elif_body.last().map(|s| s.span).unwrap_or(elif_span);
            elifs.push(ElifClause {
                condition: elif_cond,
                body: elif_body,
                span: elif_span.merge(end),
            });
        }

        let else_body = if self.is_lone_literal("else")? {
            self.lexer.advance()?;
            Some(self.parse_compound_list()?)
        } else {
            None
        };

        let fi_tok = self.expect_closing_keyword("fi", "if", start_span)?;

        Ok(CompoundCommand::IfClause {
            condition,
            then_body,
            elifs,
            else_body,
            span: start_span.merge(fi_tok.span),
        })
    }

    fn parse_while_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("while")?;
        let condition = self.parse_required_compound_list("while condition")?;
        self.expect_closing_keyword("do", "while", start_span)?;
        let body = self.parse_required_compound_list("do body")?;
        let done_tok = self.expect_closing_keyword("done", "while", start_span)?;
        Ok(CompoundCommand::WhileClause {
            condition,
            body,
            span: start_span.merge(done_tok.span),
        })
    }

    fn parse_until_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("until")?;
        let condition = self.parse_required_compound_list("until condition")?;
        self.expect_closing_keyword("do", "until", start_span)?;
        let body = self.parse_required_compound_list("do body")?;
        let done_tok = self.expect_closing_keyword("done", "until", start_span)?;
        Ok(CompoundCommand::UntilClause {
            condition,
            body,
            span: start_span.merge(done_tok.span),
        })
    }

    fn parse_for_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        self.lexer.skip_blanks()?;
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("for")?;

        self.lexer.skip_blanks()?;
        if self.options.arithmetic_for && self.lexer.peek()?.token == Token::LParen {
            return self.parse_arithmetic_for(start_span);
        }

        self.lexer.skip_blanks()?;
        let var_name = match &self.lexer.peek()?.token {
            Token::Literal(s) => s.clone(),
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: self.lexer.peek()?.token.display_name().to_string(),
                    expected: "a variable name".to_string(),
                    span: self.lexer.peek()?.span,
                });
            }
        };
        self.lexer.advance()?;
        self.skip_linebreak()?;

        let words = if self.is_lone_literal("in")? {
            self.lexer.advance()?;
            let mut word_list = Vec::new();
            while self.is_word()? {
                if let Some(w) = self.collect_word()? {
                    word_list.push(w);
                }
            }
            if self.lexer.peek()?.token == Token::Semicolon {
                self.lexer.advance()?;
            }
            self.skip_linebreak()?;
            Some(word_list)
        } else {
            if self.lexer.peek()?.token == Token::Semicolon {
                self.lexer.advance()?;
            }
            self.skip_linebreak()?;
            None
        };

        self.expect_closing_keyword("do", "for", start_span)?;
        let body = self.parse_required_compound_list("do body")?;
        let done_tok = self.expect_closing_keyword("done", "for", start_span)?;

        Ok(CompoundCommand::ForClause {
            variable: var_name,
            words,
            body,
            span: start_span.merge(done_tok.span),
        })
    }

    fn parse_case_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        self.lexer.skip_blanks()?;
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("case")?;

        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a word after 'case'".to_string(),
                span: self.lexer.peek()?.span,
            });
        }
        let case_word = self.collect_word()?.unwrap();
        self.skip_linebreak()?;

        if !self.is_lone_literal("in")? {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "'in'".to_string(),
                span: self.lexer.peek()?.span,
            });
        }
        self.lexer.advance()?;
        self.skip_linebreak()?;

        let mut arms = Vec::new();
        while !self.is_lone_literal("esac")?
            && self.lexer.peek()?.token != Token::Eof
        {
            arms.push(self.parse_case_arm()?);
            self.skip_linebreak()?;
        }

        let esac_tok = self.expect_closing_keyword("esac", "case", start_span)?;

        Ok(CompoundCommand::CaseClause {
            word: case_word,
            arms,
            span: start_span.merge(esac_tok.span),
        })
    }

    fn parse_case_arm(&mut self) -> Result<CaseArm, ParseError> {
        self.lexer.skip_blanks()?;
        let start_span = self.lexer.peek()?.span;
        self.eat(&Token::LParen)?;

        let mut patterns = Vec::new();
        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a pattern in case arm".to_string(),
                span: self.lexer.peek()?.span,
            });
        }
        patterns.push(self.collect_word()?.unwrap());

        self.lexer.skip_blanks()?;
        while self.lexer.peek()?.token == Token::Pipe {
            self.lexer.advance()?;
            if !self.is_word()? {
                return Err(ParseError::UnexpectedToken {
                    found: self.lexer.peek()?.token.display_name().to_string(),
                    expected: "a pattern after '|'".to_string(),
                    span: self.lexer.peek()?.span,
                });
            }
            patterns.push(self.collect_word()?.unwrap());
            self.lexer.skip_blanks()?;
        }

        self.expect(&Token::RParen)?;
        self.skip_linebreak()?;

        self.lexer.skip_blanks()?;
        let body = if self.lexer.peek()?.token == Token::CaseBreak
            || self.is_lone_literal("esac")?
        {
            Vec::new()
        } else {
            self.parse_compound_list()?
        };

        self.lexer.skip_blanks()?;
        let end_span = self.lexer.peek()?.span;
        let terminator = match self.lexer.peek()?.token {
            Token::CaseBreak => {
                self.lexer.advance()?;
                Some(CaseTerminator::Break)
            }
            Token::BashCaseContinue => {
                self.lexer.advance()?;
                Some(CaseTerminator::BashContinue)
            }
            Token::BashCaseFallThrough => {
                self.lexer.advance()?;
                Some(CaseTerminator::BashFallThrough)
            }
            _ => None,
        };

        Ok(CaseArm {
            patterns,
            body,
            terminator,
            span: start_span.merge(end_span),
        })
    }

    fn parse_double_bracket(&mut self) -> Result<CompoundCommand, ParseError> {
        self.lexer.skip_blanks()?;
        let start_span = self.lexer.peek()?.span;
        self.expect(&Token::BashDblLBracket)?;

        let expression = self.parse_test_expression()?;

        self.lexer.skip_blanks()?;
        if self.lexer.peek()?.token == Token::Eof {
            return Err(ParseError::UnclosedConstruct {
                keyword: "']]'".to_string(),
                opening: "[[".to_string(),
                span: start_span,
            });
        }

        let end_span = self.lexer.peek()?.span;
        self.lexer.advance()?;

        Ok(CompoundCommand::BashDoubleBracket {
            expression,
            span: start_span.merge(end_span),
        })
    }

    fn parse_subshell_or_arithmetic(&mut self) -> Result<CompoundCommand, ParseError> {
        self.lexer.skip_blanks()?;
        let start_span = self.lexer.peek()?.span;
        self.expect(&Token::LParen)?;

        if self.options.arithmetic_command && self.lexer.peek()?.token == Token::LParen
            && self.lexer.peek()?.span.start.0 == start_span.end.0
        {
            self.lexer.advance()?;
            let mut expr = String::new();
            let mut depth = 0i32;

            // Use raw API to see Blank tokens (arithmetic needs all content)
            loop {
                let tok = self.lexer.peek()?.token.clone();
                match &tok {
                    Token::RParen if depth == 0 => {
                        self.lexer.advance()?;
                        if self.lexer.peek()?.token == Token::RParen {
                            let end_span = self.lexer.peek()?.span;
                            self.lexer.advance()?;
                            let expression = parse_arith_expr(expr.trim()).map_err(|msg| {
                                ParseError::UnexpectedToken {
                                    found: msg,
                                    expected: "a valid arithmetic expression".to_string(),
                                    span: start_span.merge(end_span),
                                }
                            })?;
                            return Ok(CompoundCommand::BashArithmeticCommand {
                                expression,
                                span: start_span.merge(end_span),
                            });
                        } else {
                            expr.push(')');
                        }
                    }
                    Token::LParen => {
                        depth += 1;
                        expr.push('(');
                        self.lexer.advance()?;
                    }
                    Token::RParen => {
                        depth -= 1;
                        expr.push(')');
                        self.lexer.advance()?;
                    }
                    Token::Eof => {
                        return Err(ParseError::UnclosedConstruct {
                            keyword: "'))'".to_string(),
                            opening: "((".to_string(),
                            span: start_span,
                        });
                    }
                    Token::Blank => {
                        if !expr.is_empty() {
                            expr.push(' ');
                        }
                        self.lexer.advance()?;
                    }
                    tok if tok.is_fragment() => {
                        expr.push_str(&fragment_token_to_source(tok));
                        self.lexer.advance()?;
                    }
                    _ => {
                        if !expr.is_empty() {
                            expr.push(' ');
                        }
                        let text = tok.display_name().trim_matches('\'');
                        expr.push_str(text);
                        self.lexer.advance()?;
                    }
                }
            }
        }

        let body = self.parse_compound_list()?;
        let rparen = self.expect(&Token::RParen)?;
        Ok(CompoundCommand::Subshell {
            body,
            span: start_span.merge(rparen.span),
        })
    }

    fn parse_brace_group(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("{")?;
        let body = self.parse_compound_list()?;
        let rbrace = self.expect_closing_keyword("}", "{", start_span)?;
        Ok(CompoundCommand::BraceGroup {
            body,
            span: start_span.merge(rbrace.span),
        })
    }

    pub(super) fn parse_required_compound_list(
        &mut self,
        context: &str,
    ) -> Result<Vec<Statement>, ParseError> {
        let list = self.parse_compound_list()?;
        if list.is_empty() {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: format!("a command in {}", context),
                span: self.lexer.peek()?.span,
            });
        }
        Ok(list)
    }

    pub(super) fn parse_compound_list(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.skip_linebreak()?;

        let mut statements = Vec::new();

        if !self.can_start_command()? {
            return Ok(statements);
        }

        self.parse_list_into(&mut statements)?;

        loop {
            self.lexer.skip_blanks()?;
            if self.lexer.peek()?.token == Token::Newline {
                self.skip_newline_list()?;
            }
            if self.can_start_command()? {
                self.parse_list_into(&mut statements)?;
                continue;
            }
            break;
        }

        Ok(statements)
    }

    fn parse_arithmetic_for(
        &mut self,
        start_span: crate::span::Span,
    ) -> Result<CompoundCommand, ParseError> {
        self.lexer.skip_blanks()?;
        self.expect(&Token::LParen)?;
        self.lexer.skip_blanks()?;
        if self.lexer.peek()?.token != Token::LParen {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "'(' for arithmetic for loop".to_string(),
                span: self.lexer.peek()?.span,
            });
        }
        self.lexer.advance()?;

        let raw = self.read_arith_for_content()?;
        let parts: Vec<&str> = raw.splitn(3, ';').collect();
        let init_str = parts.first().map(|s| s.trim()).unwrap_or("");
        let cond_str = parts.get(1).map(|s| s.trim()).unwrap_or("");
        let update_str = parts.get(2).map(|s| s.trim()).unwrap_or("");

        let parse_part = |s: &str| -> Option<ArithExpr> {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(
                    crate::parser::arith_expr::parse_arith_expr(trimmed)
                        .unwrap_or_else(|_| ArithExpr::Variable(trimmed.to_string())),
                )
            }
        };

        let init = parse_part(init_str);
        let condition = parse_part(cond_str);
        let update = parse_part(update_str);

        self.lexer.skip_blanks()?;
        if self.lexer.peek()?.token == Token::Semicolon {
            self.lexer.advance()?;
        }
        self.skip_linebreak()?;

        self.expect_closing_keyword("do", "for", start_span)?;
        let body = self.parse_required_compound_list("do body")?;
        let done_tok = self.expect_closing_keyword("done", "for", start_span)?;

        Ok(CompoundCommand::BashArithmeticFor {
            init,
            condition,
            update,
            body,
            span: start_span.merge(done_tok.span),
        })
    }

    fn read_arith_for_content(&mut self) -> Result<String, ParseError> {
        let mut content = String::new();
        let mut depth = 0i32;
        // Use raw API to see Blank tokens
        loop {
            let tok = self.lexer.peek()?.token.clone();
            match &tok {
                Token::RParen if depth == 0 => {
                    self.lexer.advance()?;
                    if self.lexer.peek()?.token == Token::RParen {
                        self.lexer.advance()?;
                        return Ok(content);
                    }
                    content.push(')');
                }
                Token::LParen => {
                    depth += 1;
                    content.push('(');
                    self.lexer.advance()?;
                }
                Token::RParen => {
                    depth -= 1;
                    content.push(')');
                    self.lexer.advance()?;
                }
                Token::Semicolon => {
                    content.push(';');
                    self.lexer.advance()?;
                }
                Token::CaseBreak => {
                    content.push_str(";;");
                    self.lexer.advance()?;
                }
                Token::Eof => {
                    return Err(ParseError::UnexpectedEof {
                        expected: "'))' closing arithmetic for loop".to_string(),
                    });
                }
                Token::Blank => {
                    if !content.is_empty() && !content.ends_with(';') {
                        content.push(' ');
                    }
                    self.lexer.advance()?;
                }
                tok if tok.is_fragment() => {
                    if matches!(tok, Token::SimpleParam(_)) {
                        content.push('$');
                    }
                    content.push_str(fragment_token_to_text(tok));
                    self.lexer.advance()?;
                }
                _ => {
                    if !content.is_empty() && !content.ends_with(';') {
                        content.push(' ');
                    }
                    let text = tok.display_name().trim_matches('\'');
                    content.push_str(text);
                    self.lexer.advance()?;
                }
            }
        }
    }
}
