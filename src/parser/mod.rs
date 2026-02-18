pub(crate) mod arith_expr;
mod bash;
mod commands;
mod compound;
mod expressions;
mod helpers;
mod test_expr;
mod token_stream;

use crate::ast::*;
use crate::dialect::ParseOptions;
use crate::error::ParseError;
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use helpers::{is_keyword, keyword_display_name};
use token_stream::TokenStream;

pub use helpers::expr_span;

/// Parse a complete shell program from source text (POSIX mode).
pub fn parse(input: &str) -> Result<Program, ParseError> {
    parse_with_options(input, ParseOptions::default())
}

/// Parse a complete shell program with the given options.
pub fn parse_with_options(input: &str, options: ParseOptions) -> Result<Program, ParseError> {
    let mut parser = Parser::new(input, options)?;
    parser.parse_program()
}

/// Wrap an expression in a Statement with Sequential mode.
fn stmt(expression: Expression, span: Span) -> Statement {
    Statement {
        expression,
        mode: ExecutionMode::Sequential,
        span,
    }
}

pub(crate) struct Parser<'src> {
    pub(super) stream: TokenStream<'src>,
    pub(super) options: ParseOptions,
}

impl<'src> Parser<'src> {
    pub fn new(input: &'src str, options: ParseOptions) -> Result<Self, ParseError> {
        let lexer = Lexer::new(input, options.clone());
        let stream = TokenStream::new(lexer)?;
        Ok(Parser { stream, options })
    }

    // ================================================================
    // Helper methods
    // ================================================================

    /// Consume the current token if it matches the expected operator token.
    fn eat(&mut self, expected: &Token) -> Result<bool, ParseError> {
        if self.stream.peek()?.token == *expected {
            self.stream.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Consume the current token if it is a keyword matching the given string.
    fn eat_keyword(&mut self, keyword: &str) -> Result<bool, ParseError> {
        if is_keyword(&self.stream.peek()?.token, keyword) {
            self.stream.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Expect and consume an operator token. Returns error if not matched.
    fn expect(&mut self, expected: &Token) -> Result<SpannedToken, ParseError> {
        let peeked = self.stream.peek()?;
        if peeked.token == *expected {
            self.stream.advance()
        } else {
            Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: expected.display_name().to_string(),
                span: self.stream.peek()?.span,
            })
        }
    }

    /// Expect and consume a keyword (reserved word that comes as Word("...")).
    fn expect_keyword(&mut self, keyword: &str) -> Result<SpannedToken, ParseError> {
        if is_keyword(&self.stream.peek()?.token, keyword) {
            self.stream.advance()
        } else {
            Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: keyword_display_name(keyword),
                span: self.stream.peek()?.span,
            })
        }
    }

    /// Expect a keyword that closes a construct. Produces UnclosedConstruct
    /// error on EOF for better error messages.
    fn expect_closing_keyword(
        &mut self,
        keyword: &str,
        opening: &str,
        opening_span: Span,
    ) -> Result<SpannedToken, ParseError> {
        if is_keyword(&self.stream.peek()?.token, keyword) {
            return self.stream.advance();
        }
        if self.stream.peek()?.token == Token::Eof {
            Err(ParseError::UnclosedConstruct {
                keyword: keyword_display_name(keyword),
                opening: opening.to_string(),
                span: opening_span,
            })
        } else {
            Err(ParseError::UnexpectedToken {
                found: self.stream.peek()?.token.display_name().to_string(),
                expected: keyword_display_name(keyword),
                span: self.stream.peek()?.span,
            })
        }
    }

    fn skip_linebreak(&mut self) -> Result<(), ParseError> {
        while self.stream.peek()?.token == Token::Newline {
            self.stream.advance()?;
        }
        Ok(())
    }

    fn skip_newline_list(&mut self) -> Result<bool, ParseError> {
        if self.stream.peek()?.token != Token::Newline {
            return Ok(false);
        }
        while self.stream.peek()?.token == Token::Newline {
            self.stream.advance()?;
        }
        Ok(true)
    }

    /// Returns true if the current token is any Word (including reserved words).
    /// Use this in argument position where reserved words are just words.
    fn is_word(&mut self) -> Result<bool, ParseError> {
        Ok(matches!(self.stream.peek()?.token, Token::Word(_)))
    }

    /// Returns true if a word string is a "closing" reserved keyword that
    /// cannot start a new command. These are structure keywords that
    /// terminate or separate compound command clauses.
    fn is_closing_keyword(w: &str) -> bool {
        matches!(
            w,
            "then" | "else" | "elif" | "fi" | "do" | "done" | "esac" | "}" | "in"
        )
    }

    fn can_start_command(&mut self) -> Result<bool, ParseError> {
        let tok = &self.stream.peek()?.token;
        Ok(match tok {
            Token::Word(w) => {
                // Words can start a command UNLESS they are closing keywords
                // (then, fi, done, do, else, elif, esac, }, in)
                !Self::is_closing_keyword(w)
            }
            Token::IoNumber(_)
            | Token::LParen
            | Token::RedirectFromFile
            | Token::RedirectToFile
            | Token::HereDocOp
            | Token::HereDocStripOp
            | Token::Append
            | Token::RedirectFromFd
            | Token::RedirectToFd
            | Token::ReadWrite
            | Token::Clobber
            | Token::BashHereStringOp
            | Token::BashRedirectAllOp
            | Token::BashAppendAllOp
            | Token::BashDblLBracket => true,
            _ => false,
        })
    }

    fn is_redirect_op(&mut self) -> Result<bool, ParseError> {
        let tok = &self.stream.peek()?.token;
        Ok(tok.is_redirect_op() || matches!(tok, Token::IoNumber(_)))
    }

    /// Check if a Word token is a compound command keyword.
    fn is_compound_keyword(w: &str) -> bool {
        matches!(w, "if" | "while" | "until" | "for" | "case" | "{")
    }

    /// Check if a Word token is a keyword that starts a compound command,
    /// including Bash extensions.
    fn is_compound_start_word(&self, w: &str) -> bool {
        Self::is_compound_keyword(w) || (w == "select" && self.options.select)
    }

    /// Check if the current token starts a compound command.
    fn is_compound_start(&mut self) -> Result<bool, ParseError> {
        let tok = self.stream.peek()?.token.clone();
        Ok(match &tok {
            Token::Word(w) => self.is_compound_start_word(w),
            Token::LParen | Token::BashDblLBracket => true,
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn parse_ok(input: &str) -> Program {
        parse(input).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", input, e))
    }

    fn first_stmt(input: &str) -> Statement {
        parse_ok(input).statements.into_iter().next().unwrap()
    }

    fn first_expr(input: &str) -> Expression {
        first_stmt(input).expression
    }

    fn first_cmd(input: &str) -> Command {
        match first_expr(input) {
            Expression::Command(c) => c,
            other => panic!("expected Command, got {:?}", other),
        }
    }

    fn first_compound(input: &str) -> CompoundCommand {
        match first_expr(input) {
            Expression::Compound { body, .. } => body,
            other => panic!("expected Compound, got {:?}", other),
        }
    }

    // === Simple commands ===

    #[test]
    fn parse_single_word_command() {
        let cmd = first_cmd("ls");
        assert_eq!(cmd.arguments.len(), 1);
        assert_eq!(
            cmd.arguments[0],
            Argument::Word(Word {
                parts: vec![Fragment::Literal("ls".into())],
                span: cmd.arguments[0].span(),
            })
        );
        assert!(cmd.assignments.is_empty());
        assert!(cmd.redirects.is_empty());
    }

    #[test]
    fn parse_command_with_args() {
        let cmd = first_cmd("echo hello world");
        assert_eq!(cmd.arguments.len(), 3);
    }

    #[test]
    fn parse_assignment_only() {
        let cmd = first_cmd("FOO=bar");
        assert_eq!(cmd.assignments.len(), 1);
        assert_eq!(cmd.assignments[0].name, "FOO");
        assert!(cmd.arguments.is_empty());
    }

    #[test]
    fn parse_assignment_before_command() {
        let cmd = first_cmd("FOO=bar echo hello");
        assert_eq!(cmd.assignments.len(), 1);
        assert_eq!(cmd.arguments.len(), 2);
    }

    #[test]
    fn parse_multiple_assignments() {
        let cmd = first_cmd("A=1 B=2 cmd");
        assert_eq!(cmd.assignments.len(), 2);
        assert_eq!(cmd.arguments.len(), 1);
    }

    // === Pipelines ===

    #[test]
    fn parse_simple_pipeline() {
        assert!(matches!(
            first_expr("ls | grep foo"),
            Expression::Pipe { .. }
        ));
    }

    #[test]
    fn parse_multi_stage_pipeline() {
        let e = first_expr("a | b | c | d");
        if let Expression::Pipe { left, .. } = &e {
            if let Expression::Pipe { left, .. } = left.as_ref() {
                assert!(matches!(left.as_ref(), Expression::Pipe { .. }));
            } else {
                panic!("expected nested Pipe");
            }
        } else {
            panic!("expected Pipe");
        }
    }

    #[test]
    fn parse_negated_pipeline() {
        assert!(matches!(first_expr("! cmd"), Expression::Not(_)));
    }

    #[test]
    fn parse_negated_pipe() {
        if let Expression::Not(inner) = &first_expr("! a | b") {
            assert!(matches!(inner.as_ref(), Expression::Pipe { .. }));
        } else {
            panic!("expected Not");
        }
    }

    // === And-Or ===

    #[test]
    fn parse_and() {
        assert!(matches!(first_expr("a && b"), Expression::And { .. }));
    }

    #[test]
    fn parse_or() {
        assert!(matches!(first_expr("a || b"), Expression::Or { .. }));
    }

    #[test]
    fn parse_mixed_and_or() {
        if let Expression::Or { left, .. } = &first_expr("a && b || c") {
            assert!(matches!(left.as_ref(), Expression::And { .. }));
        } else {
            panic!("expected Or");
        }
    }

    #[test]
    fn parse_pipe_binds_tighter_than_and() {
        if let Expression::And { left, right, .. } = &first_expr("a | b && c | d") {
            assert!(matches!(left.as_ref(), Expression::Pipe { .. }));
            assert!(matches!(right.as_ref(), Expression::Pipe { .. }));
        } else {
            panic!("expected And");
        }
    }

    // === Execution modes ===

    #[test]
    fn parse_semicolon_list() {
        let prog = parse_ok("a; b");
        assert_eq!(prog.statements.len(), 2);
        assert_eq!(prog.statements[0].mode, ExecutionMode::Terminated);
        assert_eq!(prog.statements[1].mode, ExecutionMode::Sequential);
    }

    #[test]
    fn parse_background() {
        let prog = parse_ok("cmd &");
        assert_eq!(prog.statements.len(), 1);
        assert_eq!(prog.statements[0].mode, ExecutionMode::Background);
    }

    #[test]
    fn parse_background_then_foreground() {
        let prog = parse_ok("a & b");
        assert_eq!(prog.statements.len(), 2);
        assert_eq!(prog.statements[0].mode, ExecutionMode::Background);
        assert_eq!(prog.statements[1].mode, ExecutionMode::Sequential);
    }

    #[test]
    fn parse_newline_separator() {
        let prog = parse_ok("a\nb");
        assert_eq!(prog.statements.len(), 2);
    }

    // === Redirections ===

    #[test]
    fn parse_input_redirect() {
        let cmd = first_cmd("cmd < file");
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Input(_)));
    }

    #[test]
    fn parse_output_redirect() {
        let cmd = first_cmd("cmd > file");
        assert_eq!(cmd.redirects.len(), 1);
        assert!(matches!(&cmd.redirects[0].kind, RedirectKind::Output(_)));
    }

    #[test]
    fn parse_fd_redirect() {
        let cmd = first_cmd("cmd 2>&1");
        assert_eq!(cmd.redirects.len(), 1);
        assert_eq!(cmd.redirects[0].fd, Some(2));
        assert!(matches!(&cmd.redirects[0].kind, RedirectKind::DupOutput(_)));
    }

    #[test]
    fn parse_multiple_redirects() {
        let cmd = first_cmd("cmd < in > out 2>> err");
        assert_eq!(cmd.redirects.len(), 3);
    }

    // === Compound commands ===

    #[test]
    fn parse_if_then_fi() {
        assert!(matches!(
            first_compound("if true; then echo yes; fi"),
            CompoundCommand::IfClause { .. }
        ));
    }

    #[test]
    fn parse_if_then_else_fi() {
        if let CompoundCommand::IfClause { else_body, .. } =
            first_compound("if true; then echo yes; else echo no; fi")
        {
            assert!(else_body.is_some());
        } else {
            panic!("expected if clause");
        }
    }

    #[test]
    fn parse_if_elif_else_fi() {
        if let CompoundCommand::IfClause {
            elifs, else_body, ..
        } = first_compound("if a; then b; elif c; then d; elif e; then f; else g; fi")
        {
            assert_eq!(elifs.len(), 2);
            assert!(else_body.is_some());
        } else {
            panic!("expected if clause");
        }
    }

    #[test]
    fn parse_while_loop() {
        assert!(matches!(
            first_compound("while true; do echo loop; done"),
            CompoundCommand::WhileClause { .. }
        ));
    }

    #[test]
    fn parse_until_loop() {
        assert!(matches!(
            first_compound("until false; do echo loop; done"),
            CompoundCommand::UntilClause { .. }
        ));
    }

    #[test]
    fn parse_for_loop_with_list() {
        if let CompoundCommand::ForClause {
            variable, words, ..
        } = &first_compound("for i in a b c; do echo $i; done")
        {
            assert_eq!(variable, "i");
            assert_eq!(words.as_ref().unwrap().len(), 3);
        } else {
            panic!("expected for clause");
        }
    }

    #[test]
    fn parse_brace_group() {
        assert!(matches!(
            first_compound("{ echo hello; }"),
            CompoundCommand::BraceGroup { .. }
        ));
    }

    #[test]
    fn parse_subshell() {
        assert!(matches!(
            first_compound("(echo hello)"),
            CompoundCommand::Subshell { .. }
        ));
    }

    // === Here-documents ===

    #[test]
    fn parse_heredoc() {
        let cmd = first_cmd("cat <<EOF\nhello world\nEOF\n");
        assert_eq!(cmd.redirects.len(), 1);
        if let RedirectKind::HereDoc {
            delimiter, body, ..
        } = &cmd.redirects[0].kind
        {
            assert_eq!(delimiter, "EOF");
            assert_eq!(body, "hello world\n");
        } else {
            panic!("expected heredoc");
        }
    }

    // === Error cases ===

    #[test]
    fn parse_error_unexpected_token() {
        assert!(parse(";;").is_err());
    }

    #[test]
    fn parse_error_unclosed_if() {
        assert!(parse("if true; then echo yes").is_err());
    }

    #[test]
    fn parse_error_unclosed_paren() {
        assert!(parse("(echo hello").is_err());
    }

    #[test]
    fn parse_error_unclosed_brace() {
        assert!(parse("{ echo hello").is_err());
    }

    // === Edge cases ===

    #[test]
    fn parse_reserved_word_as_argument() {
        let cmd = first_cmd("echo if then else");
        assert_eq!(cmd.arguments.len(), 4);
    }

    #[test]
    fn parse_empty_input() {
        assert!(parse_ok("").statements.is_empty());
    }

    #[test]
    fn parse_only_newlines() {
        assert!(parse_ok("\n\n\n").statements.is_empty());
    }

    #[test]
    fn parse_compound_redirect() {
        if let Expression::Compound { redirects, .. } =
            &first_expr("if true; then echo yes; fi > output")
        {
            assert_eq!(redirects.len(), 1);
        } else {
            panic!("expected compound");
        }
    }

    #[test]
    fn parse_pipeline_with_newlines() {
        assert!(matches!(
            first_expr("echo hello |\ngrep h"),
            Expression::Pipe { .. }
        ));
    }

    #[test]
    fn parse_and_or_with_newlines() {
        assert!(matches!(
            first_expr("true &&\necho yes"),
            Expression::And { .. }
        ));
    }

    #[test]
    fn parse_for_with_newlines() {
        assert!(matches!(
            first_compound("for i in a b c\ndo\necho $i\ndone"),
            CompoundCommand::ForClause { .. }
        ));
    }

    #[test]
    fn parse_case_with_empty_arm() {
        if let CompoundCommand::CaseClause { arms, .. } = &first_compound("case x in\na) ;;\nesac")
        {
            assert_eq!(arms.len(), 1);
            assert!(arms[0].body.is_empty());
        } else {
            panic!("expected case clause");
        }
    }
}
