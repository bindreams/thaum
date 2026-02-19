use super::*;
use crate::token::GlobKind;

/// Helper: lex all tokens from input, including Blank tokens.
fn lex_all(input: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(input, ParseOptions::default());
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

/// Helper: lex all non-Blank tokens (for simpler assertions when we
/// don't care about whitespace).
fn lex_all_skip_blank(input: &str) -> Result<Vec<Token>, LexError> {
    Ok(lex_all(input)?
        .into_iter()
        .filter(|t| *t != Token::Blank)
        .collect())
}

// === Empty / EOF ===

#[test]
fn lex_empty_input() {
    let tokens = lex_all("").unwrap();
    assert!(tokens.is_empty());
}

#[test]
fn lex_only_whitespace() {
    let tokens = lex_all("   \t  ").unwrap();
    assert_eq!(tokens, vec![Token::Blank]);
}

// === Words (now fragment tokens) ===

#[test]
fn lex_single_word() {
    let tokens = lex_all("hello").unwrap();
    assert_eq!(tokens, vec![Token::Literal("hello".into())]);
}

#[test]
fn lex_multiple_words() {
    let tokens = lex_all("echo hello world").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Literal("echo".into()),
            Token::Blank,
            Token::Literal("hello".into()),
            Token::Blank,
            Token::Literal("world".into()),
        ]
    );
}

#[test]
fn lex_word_with_numbers() {
    let tokens = lex_all("file123").unwrap();
    assert_eq!(tokens, vec![Token::Literal("file123".into())]);
}

// === Newlines ===

#[test]
fn lex_newline() {
    let tokens = lex_all("a\nb").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Literal("a".into()),
            Token::Newline,
            Token::Literal("b".into()),
        ]
    );
}

// === Single-character operators ===

#[test]
fn lex_single_char_operators() {
    let tokens = lex_all_skip_blank("| ; & < > ( )").unwrap();
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

// === Multi-character operators ===

#[test]
fn lex_multi_char_operators() {
    let tokens = lex_all_skip_blank("&& || ;; << >> <& >& <> >|").unwrap();
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

#[test]
fn lex_dlessdash() {
    let tokens = lex_all("<<-").unwrap();
    assert_eq!(tokens, vec![Token::HereDocStripOp]);
}

#[test]
fn lex_operator_longest_prefix() {
    let tokens = lex_all("<<EOF").unwrap();
    assert_eq!(tokens, vec![Token::HereDocOp, Token::Literal("EOF".into())]);
}

#[test]
fn lex_operator_disambiguation() {
    let tokens = lex_all(">|").unwrap();
    assert_eq!(tokens, vec![Token::Clobber]);
}

// === IO_NUMBER ===

#[test]
fn lex_io_number_before_great() {
    let tokens = lex_all("2>").unwrap();
    assert_eq!(tokens, vec![Token::IoNumber(2), Token::RedirectToFile]);
}

#[test]
fn lex_io_number_before_less() {
    let tokens = lex_all("0<").unwrap();
    assert_eq!(tokens, vec![Token::IoNumber(0), Token::RedirectFromFile]);
}

#[test]
fn lex_number_with_space_is_word() {
    let tokens = lex_all("2 >").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Literal("2".into()), Token::Blank, Token::RedirectToFile]
    );
}

#[test]
fn lex_non_number_before_redirect_is_word() {
    let tokens = lex_all("abc>").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Literal("abc".into()), Token::RedirectToFile]
    );
}

// === Comments ===

#[test]
fn lex_comment_skipped() {
    let tokens = lex_all("# this is a comment").unwrap();
    // Comment is consumed as blank
    assert_eq!(tokens, vec![Token::Blank]);
}

#[test]
fn lex_comment_after_word() {
    let tokens = lex_all_skip_blank("echo hello # comment").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Literal("echo".into()), Token::Literal("hello".into())]
    );
}

#[test]
fn lex_hash_inside_word_not_comment() {
    let tokens = lex_all("foo#bar").unwrap();
    assert_eq!(tokens, vec![Token::Literal("foo#bar".into())]);
}

// === Quoting ===

#[test]
fn lex_single_quoted_word() {
    let tokens = lex_all("'hello world'").unwrap();
    assert_eq!(tokens, vec![Token::SingleQuoted("hello world".into())]);
}

#[test]
fn lex_double_quoted_word() {
    let tokens = lex_all("\"hello world\"").unwrap();
    assert_eq!(tokens, vec![Token::DoubleQuoted("hello world".into())]);
}

#[test]
fn lex_backslash_escape() {
    // \<space> in unquoted context: the backslash escapes the space,
    // making it part of the word, not a delimiter.
    let tokens = lex_all("hello\\ world").unwrap();
    // scan_literal emits "hello", then scan_backslash_escape emits "\\ " (escaped space),
    // then scan_literal emits "world" — all without Blank between them.
    assert_eq!(
        tokens,
        vec![
            Token::Literal("hello".into()),
            Token::Literal("\\ ".into()),
            Token::Literal("world".into()),
        ]
    );
}

#[test]
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

#[test]
fn lex_unterminated_single_quote() {
    let result = lex_all("'hello");
    assert!(matches!(
        result,
        Err(LexError::UnterminatedSingleQuote { .. })
    ));
}

#[test]
fn lex_unterminated_double_quote() {
    let result = lex_all("\"hello");
    assert!(matches!(
        result,
        Err(LexError::UnterminatedDoubleQuote { .. })
    ));
}

#[test]
fn lex_backtick_command_substitution() {
    let tokens = lex_all("`echo hi`").unwrap();
    assert_eq!(tokens, vec![Token::BacktickSub("echo hi".into())]);
}

#[test]
fn lex_unterminated_backtick() {
    let result = lex_all("`echo hi");
    assert!(matches!(
        result,
        Err(LexError::UnterminatedBackquote { .. })
    ));
}

// === Reserved words are NOT promoted by the lexer ===

#[test]
fn lex_reserved_words_are_just_words() {
    let tokens = lex_all_skip_blank("if then else fi").unwrap();
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

#[test]
fn lex_braces_are_just_words() {
    let tokens = lex_all_skip_blank("{ }").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Literal("{".into()), Token::Literal("}".into())]
    );
}

#[test]
fn lex_bang_is_just_a_word() {
    let tokens = lex_all("!").unwrap();
    assert_eq!(tokens, vec![Token::Literal("!".into())]);
}

// === Spans ===

#[test]
fn lex_span_tracking() {
    let mut lexer = Lexer::new("echo hello", ParseOptions::default());
    let t1 = lexer.next_token().unwrap();
    assert_eq!(t1.span, Span::new(0, 4));
    assert_eq!(t1.token, Token::Literal("echo".into()));

    let t2 = lexer.next_token().unwrap(); // Blank
    assert_eq!(t2.token, Token::Blank);

    let t3 = lexer.next_token().unwrap();
    assert_eq!(t3.span, Span::new(5, 10));
    assert_eq!(t3.token, Token::Literal("hello".into()));
}

#[test]
fn lex_span_operators() {
    let mut lexer = Lexer::new("&&||", ParseOptions::default());
    let t1 = lexer.next_token().unwrap();
    assert_eq!(t1.span, Span::new(0, 2));

    let t2 = lexer.next_token().unwrap();
    assert_eq!(t2.span, Span::new(2, 4));
}

// === Here-documents ===

#[test]
fn lex_heredoc_basic() {
    let input = "cat <<EOF\nhello world\nEOF\n";
    let mut lexer = Lexer::new(input, ParseOptions::default());

    assert_eq!(lexer.next_token().unwrap().token, Token::Literal("cat".into()));
    assert_eq!(lexer.next_token().unwrap().token, Token::Blank);
    assert_eq!(lexer.next_token().unwrap().token, Token::HereDocOp);
    assert_eq!(lexer.next_token().unwrap().token, Token::Literal("EOF".into()));

    assert_eq!(lexer.next_token().unwrap().token, Token::Newline);

    let t = lexer.next_token().unwrap();
    if let Token::HereDocBody(body) = &t.token {
        assert_eq!(body, "hello world\n");
    } else {
        panic!("expected HereDocBody, got {:?}", t.token);
    }
}

#[test]
fn lex_heredoc_strip_tabs() {
    let input = "cat <<-EOF\n\thello\n\tworld\n\tEOF\n";
    let mut lexer = Lexer::new(input, ParseOptions::default());

    lexer.next_token().unwrap(); // cat
    lexer.next_token().unwrap(); // Blank
    lexer.next_token().unwrap(); // <<-
    lexer.next_token().unwrap(); // EOF
    lexer.next_token().unwrap(); // \n

    let t = lexer.next_token().unwrap();
    if let Token::HereDocBody(body) = &t.token {
        assert_eq!(body, "hello\nworld\n");
    } else {
        panic!("expected HereDocBody, got {:?}", t.token);
    }
}

#[test]
fn lex_heredoc_unterminated() {
    let input = "cat <<EOF\nhello world\n";
    let mut lexer = Lexer::new(input, ParseOptions::default());

    lexer.next_token().unwrap(); // cat
    lexer.next_token().unwrap(); // Blank
    lexer.next_token().unwrap(); // <<
    lexer.next_token().unwrap(); // EOF

    let result = lexer.next_token();
    assert!(matches!(result, Err(LexError::UnterminatedHereDoc { .. })));
}

// === New fragment token tests ===

#[test]
fn lex_simple_param() {
    let tokens = lex_all("$VAR").unwrap();
    assert_eq!(tokens, vec![Token::SimpleParam("VAR".into())]);
}

#[test]
fn lex_brace_param() {
    let tokens = lex_all("${VAR:-default}").unwrap();
    assert_eq!(tokens, vec![Token::BraceParam("VAR:-default".into())]);
}

#[test]
fn lex_command_sub() {
    let tokens = lex_all("$(echo hello)").unwrap();
    assert_eq!(tokens, vec![Token::CommandSub("echo hello".into())]);
}

#[test]
fn lex_arith_sub() {
    let tokens = lex_all("$((1 + 2))").unwrap();
    assert_eq!(tokens, vec![Token::ArithSub("1 + 2".into())]);
}

#[test]
fn lex_word_with_expansion() {
    // test-${VAR} should be two adjacent fragment tokens with no Blank
    let tokens = lex_all("test-${VAR}").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Literal("test-".into()),
            Token::BraceParam("VAR".into()),
        ]
    );
}

#[test]
fn lex_glob_star() {
    let tokens = lex_all("*.txt").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Glob(GlobKind::Star),
            Token::Literal(".txt".into()),
        ]
    );
}

#[test]
fn lex_tilde_prefix() {
    let tokens = lex_all("~user").unwrap();
    assert_eq!(tokens, vec![Token::TildePrefix("user".into())]);
}

#[test]
fn lex_tilde_bare() {
    let tokens = lex_all_skip_blank("~ /home").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::TildePrefix(String::new()),
            Token::Literal("/home".into()),
        ]
    );
}

#[test]
fn lex_lone_dollar() {
    let tokens = lex_all_skip_blank("$ foo").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Literal("$".into()),
            Token::Literal("foo".into()),
        ]
    );
}
