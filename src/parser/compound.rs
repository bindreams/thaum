//! Compound command parsing: `if`/`while`/`until`/`for`/`case`/brace-group/
//! subshell/`select`, plus `[[ ]]` and `(( ))` dispatch.

use crate::ast::*;
use crate::error::ParseError;
use crate::token::Token;

use super::arith_expr::parse_arith_expr;
use super::helpers::*;
use super::Parser;

impl Parser {
    pub(super) fn parse_compound_command(&mut self) -> Result<CompoundCommand, ParseError> {
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_compound_command"
        );
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
        let then_body = self.parse_body("then body")?;

        let mut elifs = Vec::new();
        loop {
            self.lexer.eat_whitespace()?;
            let tok = self.lexer.peek()?.token.clone();
            if !tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "elif") {
                break;
            }
            let elif_span = self.lexer.peek()?.span;
            self.lexer.advance()?;
            let elif_cond = self.parse_required_compound_list("elif condition")?;
            self.expect_closing_keyword("then", "elif", elif_span)?;
            let elif_body = self.parse_body("elif body")?;
            let end = elif_body
                .last()
                .and_then(|line| line.last())
                .map(|s| s.span)
                .unwrap_or(elif_span);
            elifs.push(ElifClause {
                condition: elif_cond,
                body: elif_body,
                span: elif_span.merge(end),
            });
        }

        self.lexer.eat_whitespace()?;
        let tok = self.lexer.peek()?.token.clone();
        let else_body = if tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "else") {
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
        let body = self.parse_body("do body")?;
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
        let body = self.parse_body("do body")?;
        let done_tok = self.expect_closing_keyword("done", "until", start_span)?;
        Ok(CompoundCommand::UntilClause {
            condition,
            body,
            span: start_span.merge(done_tok.span),
        })
    }

    fn parse_for_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_for_clause"
        );
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("for")?;

        self.lexer.expect_whitespace()?;
        if self.options.arithmetic_for && self.lexer.peek()?.token == Token::LParen {
            return self.parse_arithmetic_for(start_span);
        }

        // No eat_whitespace: line 123 already ate
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

        let tok = self.lexer.peek()?.token.clone();
        let words = if tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "in") {
            self.lexer.advance()?;
            let mut word_list = Vec::new();
            loop {
                self.lexer.eat_whitespace()?;
                if !self.lexer.peek()?.token.is_fragment() {
                    break;
                }
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
        let body = self.parse_body("do body")?;
        let done_tok = self.expect_closing_keyword("done", "for", start_span)?;

        Ok(CompoundCommand::ForClause {
            variable: var_name,
            words,
            body,
            span: start_span.merge(done_tok.span),
        })
    }

    fn parse_case_clause(&mut self) -> Result<CompoundCommand, ParseError> {
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_case_clause"
        );
        let start_span = self.lexer.peek()?.span;
        self.expect_keyword("case")?;

        self.lexer.expect_whitespace()?;
        if !self.lexer.peek()?.token.is_fragment() {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a word after 'case'".to_string(),
                span: self.lexer.peek()?.span,
            });
        }
        let case_word = self.collect_word()?.unwrap();
        self.skip_linebreak()?;

        let tok = self.lexer.peek()?.token.clone();
        if !tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "in") {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "'in'".to_string(),
                span: self.lexer.peek()?.span,
            });
        }
        self.lexer.advance()?;
        self.skip_linebreak()?;

        let mut arms = Vec::new();
        loop {
            // No eat_whitespace: preceded by skip_linebreak (line 206/218)
            let tok = self.lexer.peek()?.token.clone();
            if tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "esac") || tok == Token::Eof {
                break;
            }
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
        // No eat_whitespace: caller's skip_linebreak already ate
        let start_span = self.lexer.peek()?.span;
        self.eat(&Token::LParen)?;

        let mut patterns = Vec::new();
        // No eat_whitespace: eat(LParen) ate WS or consumed ( (operator → no WS)
        if !self.lexer.peek()?.token.is_fragment() {
            return Err(ParseError::UnexpectedToken {
                found: self.lexer.peek()?.token.display_name().to_string(),
                expected: "a pattern in case arm".to_string(),
                span: self.lexer.peek()?.span,
            });
        }
        patterns.push(self.collect_word()?.unwrap());

        self.lexer.eat_whitespace()?;
        while self.lexer.peek()?.token == Token::Pipe {
            self.lexer.advance()?;
            // No eat_whitespace: after | (operator → LastScanned::Other)
            if !self.lexer.peek()?.token.is_fragment() {
                return Err(ParseError::UnexpectedToken {
                    found: self.lexer.peek()?.token.display_name().to_string(),
                    expected: "a pattern after '|'".to_string(),
                    span: self.lexer.peek()?.span,
                });
            }
            patterns.push(self.collect_word()?.unwrap());
            self.lexer.eat_whitespace()?;
        }

        self.expect(&Token::RParen)?;
        self.skip_linebreak()?;

        // No eat_whitespace: skip_linebreak already ate
        let tok = self.lexer.peek()?.token.clone();
        let body = if tok == Token::CaseBreak
            || tok.is_keyword(&self.lexer.peek_at_offset(1)?.token, "esac")
        {
            Vec::new()
        } else {
            self.parse_compound_list()?
        };

        self.lexer.eat_whitespace()?;
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
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_double_bracket"
        );
        let start_span = self.lexer.peek()?.span;
        self.expect(&Token::BashDblLBracket)?;

        // Enable ]] recognition in the lexer (outside [[ ]], ]] is a regular word).
        self.lexer.inside_double_bracket = true;
        let result = (|| {
            let expression = self.parse_test_expression()?;

            self.lexer.eat_whitespace()?;
            if self.lexer.peek()?.token == Token::Eof {
                return Err(ParseError::UnclosedConstruct {
                    keyword: "']]'".to_string(),
                    opening: "[[".to_string(),
                    span: start_span,
                });
            }

            let end_span = self.lexer.peek()?.span;
            self.expect(&Token::BashDblRBracket)?;

            Ok(CompoundCommand::BashDoubleBracket {
                expression,
                span: start_span.merge(end_span),
            })
        })();
        self.lexer.inside_double_bracket = false;
        result
    }

    fn parse_subshell_or_arithmetic(&mut self) -> Result<CompoundCommand, ParseError> {
        debug_assert!(
            self.lexer.peek()?.token != Token::Whitespace,
            "caller must skip whitespace before parse_subshell_or_arithmetic"
        );
        let start_span = self.lexer.peek()?.span;
        self.expect(&Token::LParen)?;

        if self.options.arithmetic_command
            && self.lexer.peek()?.token == Token::LParen
            && self.lexer.peek()?.span.start.0 == start_span.end.0
        {
            // Speculate: try (( as arithmetic. If )) is never found, rewind
            // and fall through to subshell (the second ( becomes a nested subshell).
            let arith = self.lexer.speculate(|lex| {
                lex.advance()?; // consume second (
                let mut expr = String::new();
                let mut depth = 0i32;

                loop {
                    let tok = lex.peek()?.token.clone();
                    match &tok {
                        Token::RParen if depth == 0 => {
                            lex.advance()?;
                            if lex.peek()?.token == Token::RParen {
                                let end_span = lex.peek()?.span;
                                lex.advance()?;
                                let expression = match parse_arith_expr(expr.trim()) {
                                    Ok(e) => e,
                                    Err(_) => return Ok(None), // not arithmetic
                                };
                                return Ok(Some(CompoundCommand::BashArithmeticCommand {
                                    expression,
                                    span: start_span.merge(end_span),
                                }));
                            } else {
                                expr.push(')');
                            }
                        }
                        Token::LParen => {
                            depth += 1;
                            expr.push('(');
                            lex.advance()?;
                        }
                        Token::RParen => {
                            depth -= 1;
                            expr.push(')');
                            lex.advance()?;
                        }
                        Token::Eof => return Ok(None), // no )) found
                        Token::Whitespace => {
                            if !expr.is_empty() {
                                expr.push(' ');
                            }
                            lex.advance()?;
                        }
                        tok if tok.is_fragment() => {
                            expr.push_str(&fragment_token_to_source(tok));
                            lex.advance()?;
                        }
                        _ => {
                            if matches!(tok, Token::HereDocOp | Token::HereDocStripOp) {
                                lex.cancel_pending_heredoc();
                            }
                            if !expr.is_empty() {
                                expr.push(' ');
                            }
                            let text = tok.display_name().trim_matches('\'');
                            expr.push_str(text);
                            lex.advance()?;
                        }
                    }
                }
            })?;

            if let Some(cmd) = arith {
                return Ok(cmd);
            }
            // Speculation failed — fall through to subshell.
            // The second ( is back in the buffer.
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
    ) -> Result<Vec<Line>, ParseError> {
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

    /// Parse a compound body (then-body, do-body, etc.).
    /// In bash mode, empty bodies are allowed; in POSIX mode, at least one
    /// command is required.
    pub(super) fn parse_body(&mut self, context: &str) -> Result<Vec<Line>, ParseError> {
        if self.options.empty_compound_body {
            self.parse_compound_list()
        } else {
            self.parse_required_compound_list(context)
        }
    }

    pub(super) fn parse_compound_list(&mut self) -> Result<Vec<Line>, ParseError> {
        self.skip_linebreak()?;

        let mut lines = Vec::new();

        // No eat_whitespace: skip_linebreak already ate
        let tok = self.lexer.peek()?.token.clone();
        if !tok.can_start_command(&self.lexer.peek_at_offset(1)?.token) {
            return Ok(lines);
        }

        let mut line = Vec::new();
        self.parse_list_into(&mut line)?;
        lines.push(line);

        loop {
            self.lexer.eat_whitespace()?;
            if self.lexer.peek()?.token == Token::Newline {
                self.skip_newline_list()?;
            }
            // No eat_whitespace: either skip_newline_list or line 454 already ate
            let tok = self.lexer.peek()?.token.clone();
            if tok.can_start_command(&self.lexer.peek_at_offset(1)?.token) {
                let mut line = Vec::new();
                self.parse_list_into(&mut line)?;
                lines.push(line);
                continue;
            }
            break;
        }

        Ok(lines)
    }

    fn parse_arithmetic_for(
        &mut self,
        start_span: crate::span::Span,
    ) -> Result<CompoundCommand, ParseError> {
        // No eat_whitespace: caller (parse_for_clause line 123) already ate
        self.expect(&Token::LParen)?;
        // No eat_whitespace: after ( (operator → LastScanned::Other)
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

        // No eat_whitespace: read_arith_for_content consumed )) (operators → no WS)
        if self.lexer.peek()?.token == Token::Semicolon {
            self.lexer.advance()?;
        }
        self.skip_linebreak()?;

        self.expect_closing_keyword("do", "for", start_span)?;
        let body = self.parse_body("do body")?;
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
        // Use raw API to see Whitespace tokens
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
                Token::Whitespace => {
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
                    // << / <<- inside for (( )) is a shift operator, not heredoc.
                    if matches!(tok, Token::HereDocOp | Token::HereDocStripOp) {
                        self.lexer.cancel_pending_heredoc();
                    }
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
