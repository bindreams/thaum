use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::arith_expr::parse_arith_expr;
use super::helpers::*;
use super::Parser;

impl<'src> Parser<'src> {
    pub(super) fn parse_compound_command(&mut self) -> Result<CompoundCommand, ParseError> {
        let tok = self.stream.peek()?.token.clone();
        match &tok {
            Token::Word(w) => match w.as_str() {
                "if" => self.parse_if_clause(),
                "while" => self.parse_while_clause(),
                "until" => self.parse_until_clause(),
                "for" => self.parse_for_clause(),
                "case" => self.parse_case_clause(),
                "{" => self.parse_brace_group(),
                "select" if self.options.select => self.parse_select_clause(),
                _ => Err(ParseError::UnexpectedToken {
                    found: self.stream.peek()?.token.display_name().to_string(),
                    expected: "a compound command".to_string(),
                    span: self.stream.peek()?.span,
                }),
            },
            Token::LParen => self.parse_subshell_or_arithmetic(),
            Token::BashDblLBracket => self.parse_double_bracket(),
            _ => Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a compound command".to_string(),
                span: self.stream.peek()?.span,
            }),
        }
    }

    fn parse_if_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.stream.peek()?.span;
        self.expect_keyword("if")?;

        let condition = self.parse_required_compound_list("if condition")?;
        self.expect_closing_keyword("then", "if", start_span)?;
        let then_body = self.parse_required_compound_list("then body")?;

        let mut elifs = Vec::new();
        while is_keyword(&self.stream.peek()?.token, "elif") {
            let elif_span = self.stream.peek()?.span;
            self.stream.advance()?;
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

        let else_body = if is_keyword(&self.stream.peek()?.token, "else") {
            self.stream.advance()?;
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
        let start_span = self.stream.peek()?.span;
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
        let start_span = self.stream.peek()?.span;
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
        let start_span = self.stream.peek()?.span;
        self.expect_keyword("for")?;

        // Check for arithmetic for: for (( init ; cond ; update ))
        if self.options.arithmetic_for && self.stream.peek()?.token == Token::LParen {
            return self.parse_arithmetic_for(start_span);
        }

        let var_name = match &self.stream.peek()?.token {
            Token::Word(s) => s.clone(),
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: self.stream.peek()?.token.display_name().to_string(),
                    expected: "a variable name".to_string(),
                    span: self.stream.peek()?.span,
                });
            }
        };
        self.stream.advance()?;
        self.skip_linebreak()?;

        let words = if is_keyword(&self.stream.peek()?.token, "in") {
            self.stream.advance()?;
            let mut word_list = Vec::new();
            while self.is_word()? {
                let span = self.stream.peek()?.span;
                if let Token::Word(s) = &self.stream.peek()?.token {
                    word_list.push(make_word(s.clone(), span, &self.options));
                }
                self.stream.advance()?;
            }
            if self.stream.peek()?.token == Token::Semicolon {
                self.stream.advance()?;
            }
            self.skip_linebreak()?;
            Some(word_list)
        } else {
            if self.stream.peek()?.token == Token::Semicolon {
                self.stream.advance()?;
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
        let start_span = self.stream.peek()?.span;
        self.expect_keyword("case")?;

        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a word after 'case'".to_string(),
                span: self.stream.peek()?.span,
            });
        }
        let word_span = self.stream.peek()?.span;
        let case_word = match &self.stream.peek()?.token {
            Token::Word(s) => make_word(s.clone(), word_span, &self.options),
            _ => unreachable!(),
        };
        self.stream.advance()?;
        self.skip_linebreak()?;

        if !is_keyword(&self.stream.peek()?.token, "in") {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "'in'".to_string(),
                span: self.stream.peek()?.span,
            });
        }
        self.stream.advance()?;
        self.skip_linebreak()?;

        let mut arms = Vec::new();
        while !is_keyword(&self.stream.peek()?.token, "esac")
            && self.stream.peek()?.token != Token::Eof
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
        let start_span = self.stream.peek()?.span;
        self.eat(&Token::LParen)?;

        let mut patterns = Vec::new();
        if !self.is_word()? {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "a pattern in case arm".to_string(),
                span: self.stream.peek()?.span,
            });
        }
        let span = self.stream.peek()?.span;
        if let Token::Word(s) = &self.stream.peek()?.token {
            patterns.push(make_word(s.clone(), span, &self.options));
        }
        self.stream.advance()?;

        while self.stream.peek()?.token == Token::Pipe {
            self.stream.advance()?;
            if !self.is_word()? {
                return Err(ParseError::UnexpectedToken {
                    found: self.stream.peek()?.token.display_name().to_string(),
                    expected: "a pattern after '|'".to_string(),
                    span: self.stream.peek()?.span,
                });
            }
            let span = self.stream.peek()?.span;
            if let Token::Word(s) = &self.stream.peek()?.token {
                patterns.push(make_word(s.clone(), span, &self.options));
            }
            self.stream.advance()?;
        }

        self.expect(&Token::RParen)?;
        self.skip_linebreak()?;

        let body = if self.stream.peek()?.token == Token::CaseBreak
            || is_keyword(&self.stream.peek()?.token, "esac")
        {
            Vec::new()
        } else {
            self.parse_compound_list()?
        };

        let end_span = self.stream.peek()?.span;
        let terminator = match self.stream.peek()?.token {
            Token::CaseBreak => {
                self.stream.advance()?;
                Some(CaseTerminator::Break)
            }
            Token::BashCaseContinue => {
                self.stream.advance()?;
                Some(CaseTerminator::BashContinue)
            }
            Token::BashCaseFallThrough => {
                self.stream.advance()?;
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

    /// Parse `[[ expression ]]` — extended test command (Bash).
    ///
    /// Consumes `[[`, parses the boolean expression tree via
    /// `parse_test_expression`, then expects `]]`.
    fn parse_double_bracket(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.stream.peek()?.span;
        self.expect(&Token::BashDblLBracket)?;

        let expression = self.parse_test_expression()?;

        if self.stream.peek()?.token == Token::Eof {
            return Err(ParseError::UnclosedConstruct {
                keyword: "']]'".to_string(),
                opening: "[[".to_string(),
                span: start_span,
            });
        }

        let end_span = self.stream.peek()?.span;
        self.stream.advance()?; // consume ]]

        Ok(CompoundCommand::BashDoubleBracket {
            expression,
            span: start_span.merge(end_span),
        })
    }

    /// Parse `(( ... ))` or `( ... )` -- dispatches based on next char.
    fn parse_subshell_or_arithmetic(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.stream.peek()?.span;
        self.expect(&Token::LParen)?;

        // Check if this is (( -- arithmetic command.
        // Only when the two parens are adjacent (no whitespace): `((` is arithmetic,
        // but `( (` (space-separated) is a nested subshell.
        if self.options.arithmetic_command && self.stream.peek()?.token == Token::LParen
            && self.stream.peek()?.span.start.0 == start_span.end.0
        {
            // It's (( -- consume second ( and read until ))
            self.stream.advance()?;
            let mut expr = String::new();
            let mut depth = 0i32;

            loop {
                match &self.stream.peek()?.token {
                    Token::RParen if depth == 0 => {
                        // First ), check if next is also )
                        let _inner_span = self.stream.peek()?.span;
                        self.stream.advance()?;
                        if self.stream.peek()?.token == Token::RParen {
                            let end_span = self.stream.peek()?.span;
                            self.stream.advance()?;
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
                            // Single ), part of expression
                            expr.push(')');
                            // Continue
                        }
                    }
                    Token::LParen => {
                        depth += 1;
                        expr.push('(');
                        self.stream.advance()?;
                    }
                    Token::RParen => {
                        depth -= 1;
                        expr.push(')');
                        self.stream.advance()?;
                    }
                    Token::Eof => {
                        return Err(ParseError::UnclosedConstruct {
                            keyword: "'))'".to_string(),
                            opening: "((".to_string(),
                            span: start_span,
                        });
                    }
                    Token::Word(s) => {
                        if !expr.is_empty() {
                            expr.push(' ');
                        }
                        expr.push_str(s);
                        self.stream.advance()?;
                    }
                    _ => {
                        if !expr.is_empty() {
                            expr.push(' ');
                        }
                        let text = self.stream.peek()?.token.display_name().trim_matches('\'');
                        expr.push_str(text);
                        self.stream.advance()?;
                    }
                }
            }
        }

        // Regular subshell
        let body = self.parse_compound_list()?;
        let rparen = self.expect(&Token::RParen)?;
        Ok(CompoundCommand::Subshell {
            body,
            span: start_span.merge(rparen.span),
        })
    }

    fn parse_brace_group(&mut self) -> Result<CompoundCommand, ParseError> {
        let start_span = self.stream.peek()?.span;
        self.expect_keyword("{")?;
        let body = self.parse_compound_list()?;
        let rbrace = self.expect_closing_keyword("}", "{", start_span)?;
        Ok(CompoundCommand::BraceGroup {
            body,
            span: start_span.merge(rbrace.span),
        })
    }

    /// Parse a compound list that must contain at least one statement.
    pub(super) fn parse_required_compound_list(
        &mut self,
        context: &str,
    ) -> Result<Vec<Statement>, ParseError> {
        let list = self.parse_compound_list()?;
        if list.is_empty() {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: format!("a command in {}", context),
                span: self.stream.peek()?.span,
            });
        }
        Ok(list)
    }

    /// Parse a compound list (inside compound commands). Returns Vec<Statement>.
    pub(super) fn parse_compound_list(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.skip_linebreak()?;

        let mut statements = Vec::new();

        if !self.can_start_command()? {
            return Ok(statements);
        }

        self.parse_list_into(&mut statements)?;

        loop {
            // After parse_list_into, the next token could be:
            // - Newline (statement separator — skip and continue)
            // - A command start (heredoc consumed the Newline — continue directly)
            // - A closing keyword (fi, done, etc. — break)
            if self.stream.peek()?.token == Token::Newline {
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

    /// Parse `for (( init ; cond ; update )); do body; done`.
    fn parse_arithmetic_for(
        &mut self,
        start_span: crate::span::Span,
    ) -> Result<CompoundCommand, ParseError> {
        // Consume (( — first ( is current token
        self.expect(&Token::LParen)?;
        if self.stream.peek()?.token != Token::LParen {
            return Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: "'(' for arithmetic for loop".to_string(),
                span: self.stream.peek()?.span,
            });
        }
        self.stream.advance()?; // consume second (

        // Read everything between (( and )) as a raw string, then split on ;
        let raw = self.read_arith_for_content()?;
        let parts: Vec<&str> = raw.splitn(3, ';').collect();
        let init_str = parts.first().map(|s| s.trim()).unwrap_or("");
        let cond_str = parts.get(1).map(|s| s.trim()).unwrap_or("");
        let update_str = parts.get(2).map(|s| s.trim()).unwrap_or("");

        // Parse each part (empty strings → None)
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

        // Optional semicolon after ))
        if self.stream.peek()?.token == Token::Semicolon {
            self.stream.advance()?;
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

    /// Read the raw content between `((` and `))` in an arithmetic for loop.
    /// Returns the full string, which the caller splits on `;`.
    fn read_arith_for_content(&mut self) -> Result<String, ParseError> {
        let mut content = String::new();
        let mut depth = 0i32;
        loop {
            match &self.stream.peek()?.token {
                Token::RParen if depth == 0 => {
                    self.stream.advance()?; // first )
                    if self.stream.peek()?.token == Token::RParen {
                        self.stream.advance()?; // second )
                        return Ok(content);
                    }
                    content.push(')');
                }
                Token::LParen => {
                    depth += 1;
                    content.push('(');
                    self.stream.advance()?;
                }
                Token::RParen => {
                    depth -= 1;
                    content.push(')');
                    self.stream.advance()?;
                }
                Token::Semicolon => {
                    content.push(';');
                    self.stream.advance()?;
                }
                Token::CaseBreak => {
                    // `;;` tokenized as one token — emit as two semicolons
                    content.push_str(";;");
                    self.stream.advance()?;
                }
                Token::Eof => {
                    return Err(ParseError::UnexpectedEof {
                        expected: "'))' closing arithmetic for loop".to_string(),
                    });
                }
                Token::Word(s) => {
                    if !content.is_empty() && !content.ends_with(';') {
                        content.push(' ');
                    }
                    content.push_str(s);
                    self.stream.advance()?;
                }
                _ => {
                    if !content.is_empty() && !content.ends_with(';') {
                        content.push(' ');
                    }
                    let text = self.stream.peek()?.token.display_name().trim_matches('\'');
                    content.push_str(text);
                    self.stream.advance()?;
                }
            }
        }
    }
}
