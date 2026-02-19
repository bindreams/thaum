use super::*;

/// Helper: lex all tokens from input.
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

// === Empty / EOF ===

#[test]
fn lex_empty_input() {
    let tokens = lex_all("").unwrap();
    assert!(tokens.is_empty());
}

#[test]
fn lex_only_whitespace() {
    let tokens = lex_all("   \t  ").unwrap();
    assert!(tokens.is_empty());
}

// === Words ===

#[test]
fn lex_single_word() {
    let tokens = lex_all("hello").unwrap();
    assert_eq!(tokens, vec![Token::Word("hello".into())]);
}

#[test]
fn lex_multiple_words() {
    let tokens = lex_all("echo hello world").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Word("echo".into()),
            Token::Word("hello".into()),
            Token::Word("world".into()),
        ]
    );
}

#[test]
fn lex_word_with_numbers() {
    let tokens = lex_all("file123").unwrap();
    assert_eq!(tokens, vec![Token::Word("file123".into())]);
}

// === Newlines ===

#[test]
fn lex_newline() {
    let tokens = lex_all("a\nb").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Word("a".into()),
            Token::Newline,
            Token::Word("b".into()),
        ]
    );
}

// === Single-character operators ===

#[test]
fn lex_single_char_operators() {
    let tokens = lex_all("| ; & < > ( )").unwrap();
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
    let tokens = lex_all("&& || ;; << >> <& >& <> >|").unwrap();
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
    // `<<` should not be split into `<` `<`
    let tokens = lex_all("<<EOF").unwrap();
    assert_eq!(tokens, vec![Token::HereDocOp, Token::Word("EOF".into())]);
}

#[test]
fn lex_operator_disambiguation() {
    // `>|` is Clobber, not Great + Pipe
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
    // Space between number and operator → just a word
    let tokens = lex_all("2 >").unwrap();
    assert_eq!(tokens, vec![Token::Word("2".into()), Token::RedirectToFile]);
}

#[test]
fn lex_non_number_before_redirect_is_word() {
    let tokens = lex_all("abc>").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Word("abc".into()), Token::RedirectToFile]
    );
}

// === Comments ===

#[test]
fn lex_comment_skipped() {
    let tokens = lex_all("# this is a comment").unwrap();
    assert!(tokens.is_empty());
}

#[test]
fn lex_comment_after_word() {
    let tokens = lex_all("echo hello # comment").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Word("echo".into()), Token::Word("hello".into())]
    );
}

#[test]
fn lex_hash_inside_word_not_comment() {
    let tokens = lex_all("foo#bar").unwrap();
    assert_eq!(tokens, vec![Token::Word("foo#bar".into())]);
}

// === Quoting ===

#[test]
fn lex_single_quoted_word() {
    let tokens = lex_all("'hello world'").unwrap();
    assert_eq!(tokens, vec![Token::Word("'hello world'".into())]);
}

#[test]
fn lex_double_quoted_word() {
    let tokens = lex_all("\"hello world\"").unwrap();
    assert_eq!(tokens, vec![Token::Word("\"hello world\"".into())]);
}

#[test]
fn lex_backslash_escape() {
    let tokens = lex_all("hello\\ world").unwrap();
    assert_eq!(tokens, vec![Token::Word("hello\\ world".into())]);
}

#[test]
fn lex_mixed_quoting() {
    let tokens = lex_all("he'llo '\"wor\"ld").unwrap();
    assert_eq!(tokens, vec![Token::Word("he'llo '\"wor\"ld".into())]);
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
    assert_eq!(tokens, vec![Token::Word("`echo hi`".into())]);
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
    // The lexer never promotes reserved words — that's the parser's job
    let tokens = lex_all("if then else fi").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Word("if".into()),
            Token::Word("then".into()),
            Token::Word("else".into()),
            Token::Word("fi".into()),
        ]
    );
}

#[test]
fn lex_braces_are_just_words() {
    let tokens = lex_all("{ }").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Word("{".into()), Token::Word("}".into())]
    );
}

#[test]
fn lex_bang_is_just_a_word() {
    let tokens = lex_all("!").unwrap();
    assert_eq!(tokens, vec![Token::Word("!".into())]);
}

// === Spans ===

#[test]
fn lex_span_tracking() {
    let mut lexer = Lexer::new("echo hello", ParseOptions::default());
    let t1 = lexer.next_token().unwrap();
    assert_eq!(t1.span, Span::new(0, 4));
    assert_eq!(t1.token, Token::Word("echo".into()));

    let t2 = lexer.next_token().unwrap();
    assert_eq!(t2.span, Span::new(5, 10));
    assert_eq!(t2.token, Token::Word("hello".into()));
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

    assert_eq!(lexer.next_token().unwrap().token, Token::Word("cat".into()));
    assert_eq!(lexer.next_token().unwrap().token, Token::HereDocOp);
    assert_eq!(lexer.next_token().unwrap().token, Token::Word("EOF".into()));

    // Newline triggers heredoc body reading
    assert_eq!(lexer.next_token().unwrap().token, Token::Newline);

    // Body appears as a HereDocBody token after the newline
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
    lexer.next_token().unwrap(); // <<
    lexer.next_token().unwrap(); // EOF

    // Newline triggers heredoc read, which fails because EOF delimiter is never found
    let result = lexer.next_token();
    assert!(matches!(result, Err(LexError::UnterminatedHereDoc { .. })));
}
