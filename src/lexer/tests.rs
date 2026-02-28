//! Lexer unit tests covering fragments, operators, whitespace significance,
//! heredocs, quoting, spans, and the buffered peek/advance/speculate API.

use super::*;
use crate::token::GlobKind;

testutil::default_labels!(lex);

/// Helper: lex all tokens from input, including Whitespace tokens.
fn lex_all(input: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::from_str(input, ShellOptions::default());
    let mut tokens = Vec::new();
    loop {
        let st = lexer.next_token()?;
        if st.token == Token::Eof {
            break;
        }
        tokens.push(st.token);
    }
    Ok(tokens)
}

/// Helper: lex all non-Whitespace tokens (for simpler assertions when we
/// don't care about whitespace).
fn lex_all_skip_whitespace(input: &str) -> Result<Vec<Token>, LexError> {
    Ok(lex_all(input)?
        .into_iter()
        .filter(|t| *t != Token::Whitespace)
        .collect())
}

// Empty / EOF ---------------------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_empty_input() {
    let tokens = lex_all("").unwrap();
    assert!(tokens.is_empty());
}

#[testutil::test]
fn lex_only_whitespace() {
    // All whitespace at start of input — no preceding fragment, so suppressed.
    let tokens = lex_all("   \t  ").unwrap();
    assert_eq!(tokens, vec![]);
}

// Words (now fragment tokens) -----------------------------------------------------------------------------------------

#[testutil::test]
fn lex_single_word() {
    let tokens = lex_all("hello").unwrap();
    assert_eq!(tokens, vec![Token::Literal("hello".into())]);
}

#[testutil::test]
fn lex_multiple_words() {
    let tokens = lex_all("echo hello world").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Literal("echo".into()),
            Token::Whitespace,
            Token::Literal("hello".into()),
            Token::Whitespace,
            Token::Literal("world".into()),
        ]
    );
}

#[testutil::test]
fn lex_word_with_numbers() {
    let tokens = lex_all("file123").unwrap();
    assert_eq!(tokens, vec![Token::Literal("file123".into())]);
}

// Newlines ------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_newline() {
    let tokens = lex_all("a\nb").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Literal("a".into()), Token::Newline, Token::Literal("b".into()),]
    );
}

// Single-character operators ------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_single_char_operators() {
    let tokens = lex_all_skip_whitespace("| ; & < > ( )").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Pipe,
            Token::Semicolon,
            Token::Ampersand,
            Token::RedirectFromFile,
            Token::RedirectToFile,
            Token::LParen,
            Token::RParen,
        ]
    );
}

// Multi-character operators -------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_multi_char_operators() {
    let tokens = lex_all_skip_whitespace("&& || ;; << >> <& >& <> >|").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::AndIf,
            Token::OrIf,
            Token::CaseBreak,
            Token::HereDocOp,
            Token::Append,
            Token::RedirectFromFd,
            Token::RedirectToFd,
            Token::ReadWrite,
            Token::Clobber,
        ]
    );
}

#[testutil::test]
fn lex_dlessdash() {
    let tokens = lex_all("<<-").unwrap();
    assert_eq!(tokens, vec![Token::HereDocStripOp]);
}

#[testutil::test]
fn lex_operator_longest_prefix() {
    let tokens = lex_all("<<EOF").unwrap();
    assert_eq!(tokens, vec![Token::HereDocOp, Token::Literal("EOF".into())]);
}

#[testutil::test]
fn lex_operator_disambiguation() {
    let tokens = lex_all(">|").unwrap();
    assert_eq!(tokens, vec![Token::Clobber]);
}

// IO_NUMBER -----------------------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_io_number_before_great() {
    let tokens = lex_all("2>").unwrap();
    assert_eq!(tokens, vec![Token::IoNumber(2), Token::RedirectToFile]);
}

#[testutil::test]
fn lex_io_number_before_less() {
    let tokens = lex_all("0<").unwrap();
    assert_eq!(tokens, vec![Token::IoNumber(0), Token::RedirectFromFile]);
}

#[testutil::test]
fn lex_number_with_space_is_word() {
    let tokens = lex_all("2 >").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Literal("2".into()), Token::Whitespace, Token::RedirectToFile]
    );
}

#[testutil::test]
fn lex_non_number_before_redirect_is_word() {
    let tokens = lex_all("abc>").unwrap();
    assert_eq!(tokens, vec![Token::Literal("abc".into()), Token::RedirectToFile]);
}

// Comments ------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_comment_skipped() {
    // Comment at start of input — no preceding fragment, so suppressed.
    let tokens = lex_all("# this is a comment").unwrap();
    assert_eq!(tokens, vec![]);
}

#[testutil::test]
fn lex_comment_after_word() {
    let tokens = lex_all_skip_whitespace("echo hello # comment").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Literal("echo".into()), Token::Literal("hello".into())]
    );
}

#[testutil::test]
fn lex_hash_inside_word_not_comment() {
    let tokens = lex_all("foo#bar").unwrap();
    assert_eq!(tokens, vec![Token::Literal("foo#bar".into())]);
}

// Whitespace suppression ----------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_leading_whitespace_suppressed() {
    // No preceding fragment → suppressed.
    let tokens = lex_all("  echo").unwrap();
    assert_eq!(tokens, vec![Token::Literal("echo".into())]);
}

#[testutil::test]
fn lex_whitespace_after_operator_suppressed() {
    let tokens = lex_all("; echo").unwrap();
    assert_eq!(tokens, vec![Token::Semicolon, Token::Literal("echo".into())]);
}

#[testutil::test]
fn lex_whitespace_after_newline_suppressed() {
    let tokens = lex_all("\n echo").unwrap();
    assert_eq!(tokens, vec![Token::Newline, Token::Literal("echo".into())]);
}

#[testutil::test]
fn lex_whitespace_between_words_emitted() {
    let tokens = lex_all("a b").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Literal("a".into()),
            Token::Whitespace,
            Token::Literal("b".into()),
        ]
    );
}

#[testutil::test]
fn lex_process_sub_after_suppressed_whitespace() {
    // Whitespace after `;` is suppressed (no token emitted), but
    // last_scanned is still Whitespace, so `<(` is recognized as
    // process substitution, not a redirect operator.
    use crate::dialect::Dialect;
    let mut lexer = Lexer::from_str("; <(ls)", Dialect::Bash.options());
    assert_eq!(lexer.next_token().unwrap().token, Token::Semicolon);
    // Next should be a process substitution fragment, not RedirectFromFile
    let tok = lexer.next_token().unwrap().token;
    assert!(
        matches!(tok, Token::BashProcessSub { direction: '<', .. }),
        "expected BashProcessSub, got {:?}",
        tok
    );
}

// Quoting -------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_single_quoted_word() {
    let tokens = lex_all("'hello world'").unwrap();
    assert_eq!(tokens, vec![Token::SingleQuoted("hello world".into())]);
}

#[testutil::test]
fn lex_double_quoted_word() {
    let tokens = lex_all("\"hello world\"").unwrap();
    assert_eq!(tokens, vec![Token::DoubleQuoted("hello world".into())]);
}

#[testutil::test]
fn lex_backslash_escape() {
    // \<space> in unquoted context: the backslash escapes the space,
    // making it part of the word, not a delimiter.
    let tokens = lex_all("hello\\ world").unwrap();
    // scan_literal emits "hello", then scan_backslash_escape emits "\\ " (escaped space),
    // then scan_literal emits "world" — all without Whitespace between them.
    assert_eq!(
        tokens,
        vec![
            Token::Literal("hello".into()),
            Token::Literal("\\ ".into()),
            Token::Literal("world".into()),
        ]
    );
}

#[testutil::test]
fn lex_mixed_quoting() {
    // he'llo '"wor"ld — one word with mixed quoting
    let tokens = lex_all("he'llo '\"wor\"ld").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Literal("he".into()),
            Token::SingleQuoted("llo ".into()),
            Token::DoubleQuoted("wor".into()),
            Token::Literal("ld".into()),
        ]
    );
}

#[testutil::test]
fn lex_unterminated_single_quote() {
    let result = lex_all("'hello");
    assert!(matches!(result, Err(LexError::UnterminatedSingleQuote { .. })));
}

#[testutil::test]
fn lex_unterminated_double_quote() {
    let result = lex_all("\"hello");
    assert!(matches!(result, Err(LexError::UnterminatedDoubleQuote { .. })));
}

#[testutil::test]
fn lex_backtick_command_substitution() {
    let tokens = lex_all("`echo hi`").unwrap();
    assert_eq!(tokens, vec![Token::BacktickSub("echo hi".into())]);
}

#[testutil::test]
fn lex_unterminated_backtick() {
    let result = lex_all("`echo hi");
    assert!(matches!(result, Err(LexError::UnterminatedBackquote { .. })));
}

// Reserved words are NOT promoted by the lexer ------------------------------------------------------------------------

#[testutil::test]
fn lex_reserved_words_are_just_words() {
    let tokens = lex_all_skip_whitespace("if then else fi").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Literal("if".into()),
            Token::Literal("then".into()),
            Token::Literal("else".into()),
            Token::Literal("fi".into()),
        ]
    );
}

#[testutil::test]
fn lex_braces_are_just_words() {
    let tokens = lex_all_skip_whitespace("{ }").unwrap();
    assert_eq!(tokens, vec![Token::Literal("{".into()), Token::Literal("}".into())]);
}

#[testutil::test]
fn lex_bang_is_just_a_word() {
    let tokens = lex_all("!").unwrap();
    assert_eq!(tokens, vec![Token::Literal("!".into())]);
}

// Spans ---------------------------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_span_tracking() {
    let mut lexer = Lexer::from_str("echo hello", ShellOptions::default());
    let t1 = lexer.next_token().unwrap();
    assert_eq!(t1.span, Span::new(0, 4));
    assert_eq!(t1.token, Token::Literal("echo".into()));

    let t2 = lexer.next_token().unwrap(); // Whitespace
    assert_eq!(t2.token, Token::Whitespace);

    let t3 = lexer.next_token().unwrap();
    assert_eq!(t3.span, Span::new(5, 10));
    assert_eq!(t3.token, Token::Literal("hello".into()));
}

#[testutil::test]
fn lex_span_operators() {
    let mut lexer = Lexer::from_str("&&||", ShellOptions::default());
    let t1 = lexer.next_token().unwrap();
    assert_eq!(t1.span, Span::new(0, 2));

    let t2 = lexer.next_token().unwrap();
    assert_eq!(t2.span, Span::new(2, 4));
}

// Here-documents ------------------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_heredoc_basic() {
    let input = "cat <<EOF\nhello world\nEOF\n";
    let mut lexer = Lexer::from_str(input, ShellOptions::default());

    assert_eq!(lexer.next_token().unwrap().token, Token::Literal("cat".into()));
    assert_eq!(lexer.next_token().unwrap().token, Token::Whitespace);
    assert_eq!(lexer.next_token().unwrap().token, Token::HereDocOp);
    assert_eq!(lexer.next_token().unwrap().token, Token::Literal("EOF".into()));

    // Newline triggers heredoc body reading into side queue (not buffer).
    assert_eq!(lexer.next_token().unwrap().token, Token::Newline);
    assert_eq!(lexer.take_heredoc_body().unwrap(), "hello world\n");
}

#[testutil::test]
fn lex_heredoc_strip_tabs() {
    let input = "cat <<-EOF\n\thello\n\tworld\n\tEOF\n";
    let mut lexer = Lexer::from_str(input, ShellOptions::default());

    lexer.next_token().unwrap(); // cat
    lexer.next_token().unwrap(); // Whitespace
    lexer.next_token().unwrap(); // <<-
    lexer.next_token().unwrap(); // EOF
    lexer.next_token().unwrap(); // \n (triggers body reading into side queue)

    assert_eq!(lexer.take_heredoc_body().unwrap(), "hello\nworld\n");
}

#[testutil::test]
fn lex_heredoc_unterminated() {
    let input = "cat <<EOF\nhello world\n";
    let mut lexer = Lexer::from_str(input, ShellOptions::default());

    lexer.next_token().unwrap(); // cat
    lexer.next_token().unwrap(); // Whitespace
    lexer.next_token().unwrap(); // <<
    lexer.next_token().unwrap(); // EOF

    let result = lexer.next_token();
    assert!(matches!(result, Err(LexError::UnterminatedHereDoc { .. })));
}

// New fragment token tests --------------------------------------------------------------------------------------------

#[testutil::test]
fn lex_simple_param() {
    let tokens = lex_all("$VAR").unwrap();
    assert_eq!(tokens, vec![Token::SimpleParam("VAR".into())]);
}

#[testutil::test]
fn lex_brace_param() {
    let tokens = lex_all("${VAR:-default}").unwrap();
    assert_eq!(tokens, vec![Token::BraceParam("VAR:-default".into())]);
}

#[testutil::test]
fn lex_command_sub() {
    let tokens = lex_all("$(echo hello)").unwrap();
    assert_eq!(tokens, vec![Token::CommandSub("echo hello".into())]);
}

#[testutil::test]
fn lex_arith_sub() {
    let tokens = lex_all("$((1 + 2))").unwrap();
    assert_eq!(tokens, vec![Token::ArithSub("1 + 2".into())]);
}

#[testutil::test]
fn lex_word_with_expansion() {
    // test-${VAR} should be two adjacent fragment tokens with no Whitespace
    let tokens = lex_all("test-${VAR}").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Literal("test-".into()), Token::BraceParam("VAR".into()),]
    );
}

#[testutil::test]
fn lex_glob_star() {
    let tokens = lex_all("*.txt").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Glob(GlobKind::Star), Token::Literal(".txt".into()),]
    );
}

#[testutil::test]
fn lex_tilde_prefix() {
    let tokens = lex_all("~user").unwrap();
    assert_eq!(tokens, vec![Token::TildePrefix("user".into())]);
}

#[testutil::test]
fn lex_tilde_bare() {
    let tokens = lex_all_skip_whitespace("~ /home").unwrap();
    assert_eq!(
        tokens,
        vec![Token::TildePrefix(String::new()), Token::Literal("/home".into()),]
    );
}

#[testutil::test]
fn lex_lone_dollar() {
    let tokens = lex_all_skip_whitespace("$ foo").unwrap();
    assert_eq!(tokens, vec![Token::Literal("$".into()), Token::Literal("foo".into()),]);
}

// Token-level buffered API (migrated from token_stream_tests) ---------------------------------------------------------

fn make_lexer(input: &str) -> Lexer {
    Lexer::from_str(input, ShellOptions::default())
}

#[testutil::test]
fn peek_returns_first_token() {
    let mut s = make_lexer("echo hello");
    assert_eq!(s.peek().unwrap().token, Token::Literal("echo".into()));
}

#[testutil::test]
fn peek_is_idempotent() {
    let mut s = make_lexer("echo hello");
    let t1 = s.peek().unwrap().token.clone();
    let t2 = s.peek().unwrap().token.clone();
    assert_eq!(t1, t2);
}

#[testutil::test]
fn advance_returns_peeked() {
    let mut s = make_lexer("echo hello");
    let peeked = s.peek().unwrap().token.clone();
    let advanced = s.advance().unwrap().token;
    assert_eq!(peeked, advanced);
}

#[testutil::test]
fn advance_then_skip_whitespace() {
    let mut s = make_lexer("echo hello");
    s.advance().unwrap();
    s.skip_whitespace().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Literal("hello".into()));
}

#[testutil::test]
fn advance_past_eof() {
    let mut s = make_lexer("x");
    s.advance().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Eof);
    assert_eq!(s.advance().unwrap().token, Token::Eof);
}

#[testutil::test]
fn peek_sees_whitespace_without_skip() {
    let mut s = make_lexer("a b");
    s.advance().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Whitespace);
}

#[testutil::test]
fn skip_whitespace_then_peek() {
    let mut s = make_lexer("a b");
    s.advance().unwrap();
    s.skip_whitespace().unwrap();
    assert_eq!(s.peek().unwrap().token, Token::Literal("b".into()));
}

#[testutil::test]
fn speculate_rewinds_on_none() {
    let mut s = make_lexer("a b c");
    let result: Option<()> = s
        .speculate(|s| {
            s.advance()?;
            s.skip_whitespace()?;
            s.advance()?;
            Ok(None)
        })
        .unwrap();
    assert!(result.is_none());
    assert_eq!(s.peek().unwrap().token, Token::Literal("a".into()));
}

#[testutil::test]
fn speculate_keeps_position_on_some() {
    let mut s = make_lexer("a b c");
    let result = s
        .speculate(|s| {
            s.advance()?;
            s.skip_whitespace()?;
            Ok(Some("found"))
        })
        .unwrap();
    assert_eq!(result, Some("found"));
    assert_eq!(s.peek().unwrap().token, Token::Literal("b".into()));
}

#[testutil::test]
fn empty_input_peek() {
    let mut s = make_lexer("");
    assert_eq!(s.peek().unwrap().token, Token::Eof);
}

// Speculation keeps tokens in buffer ----------------------------------------------------------------------------------

#[testutil::test]
fn speculate_tokens_stay_in_buffer() {
    // Speculate past a << operator and heredoc. On rewind, buf_pos moves
    // back but the scanned tokens stay in the buffer. Re-reading gives
    // the same token sequence — no re-scanning needed.
    // NOTE: HereDocBody no longer appears in the buffer; bodies go into
    // the side queue. But the other tokens (operator, delimiter, newline)
    // must survive speculation.
    let mut s = make_lexer("<<EOF\nhello\nEOF\n");
    let result: Option<()> = s
        .speculate(|s| {
            assert_eq!(s.advance().unwrap().token, Token::HereDocOp);
            assert_eq!(s.advance().unwrap().token, Token::Literal("EOF".into()));
            assert_eq!(s.advance().unwrap().token, Token::Newline);
            // Body is in the side queue, not the buffer
            // Rewind
            Ok(None)
        })
        .unwrap();
    assert!(result.is_none());
    // After rewind: same tokens are re-read from buffer
    assert_eq!(s.advance().unwrap().token, Token::HereDocOp);
    assert_eq!(s.advance().unwrap().token, Token::Literal("EOF".into()));
    assert_eq!(s.advance().unwrap().token, Token::Newline);
    // Body was read during the first speculative pass and is in the queue
    assert_eq!(s.take_heredoc_body().unwrap(), "hello\n");
}
