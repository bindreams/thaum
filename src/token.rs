use crate::span::Span;

/// A token with its source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// All token types recognized by the shell lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    // === Value-carrying tokens ===
    /// A shell word (may contain quotes, expansions — still raw at this stage).
    Word(String),
    /// An IO_NUMBER: a digit sequence immediately preceding `<` or `>`.
    IoNumber(i32),

    // === Newline (semantically significant in shell) ===
    Newline,

    // === Reserved words ===
    // TODO: These variants are never produced by the lexer (it always emits Word).
    // They're only used by reserved_word_from_str() and display_name(). Consider
    // removing them and using string constants instead.
    If,
    Then,
    Else,
    Elif,
    Fi,
    Do,
    Done,
    Case,
    Esac,
    While,
    Until,
    For,
    In,
    /// `{` — brace group open.
    LBrace,
    /// `}` — brace group close.
    RBrace,
    /// `!` — pipeline negation.
    Bang,

    // === Multi-character operators ===
    /// `&&` — logical AND (POSIX: `AND_IF`).
    AndIf,
    /// `||` — logical OR (POSIX: `OR_IF`).
    OrIf,
    /// `;;` — case arm break (POSIX: `DSEMI`).
    CaseBreak,
    /// `<<` — here-document (POSIX: `DLESS`).
    HereDocOp,
    /// `>>` — append output to file (POSIX: `DGREAT`).
    Append,
    /// `<&` — duplicate input file descriptor (POSIX: `LESSAND`).
    RedirectFromFd,
    /// `>&` — duplicate output file descriptor (POSIX: `GREATAND`).
    RedirectToFd,
    /// `<>` — open for read-write (POSIX: `LESSGREAT`).
    ReadWrite,
    /// `>|` — force-overwrite output, ignoring `noclobber` (POSIX: `CLOBBER`).
    Clobber,
    /// `<<-` — here-document with leading tab stripping (POSIX: `DLESSDASH`).
    HereDocStripOp,

    // === Bash extensions ===
    /// `<<<` — here-string (Bash).
    BashHereStringOp,
    /// `&>` — redirect stdout+stderr to file (Bash).
    BashRedirectAllOp,
    /// `&>>` — append stdout+stderr to file (Bash).
    BashAppendAllOp,
    /// `[[` — extended test open (Bash).
    BashDblLBracket,
    /// `]]` — extended test close (Bash).
    BashDblRBracket,
    /// `;&` — case fall-through: execute next arm without testing (Bash).
    BashCaseContinue,
    /// `;;&` — case fall-through: test next pattern (Bash).
    BashCaseFallThrough,
    /// `function` keyword (Bash).
    BashFunction,
    /// `select` keyword (Bash).
    BashSelect,
    /// `coproc` keyword (Bash).
    BashCoproc,
    /// `|&` — pipe stdout+stderr (Bash).
    BashPipeAmpersand,

    // === Single-character operators ===
    /// `|` — pipeline.
    Pipe,
    /// `;` — command terminator.
    Semicolon,
    /// `&` — background execution.
    Ampersand,
    /// `<` — redirect input from file (POSIX: `LESS`).
    RedirectFromFile,
    /// `>` — redirect output to file (POSIX: `GREAT`).
    RedirectToFile,
    /// `(` — subshell/grouping open.
    LParen,
    /// `)` — subshell/grouping close.
    RParen,

    // === Special ===
    /// Here-document body — emitted by the lexer after reading the body.
    /// Appears in the token stream after the `Newline` that triggered the read.
    HereDocBody(String),
    Eof,
}

impl Token {
    /// Returns `true` if this token is a reserved word.
    pub fn is_reserved_word(&self) -> bool {
        matches!(
            self,
            Token::If
                | Token::Then
                | Token::Else
                | Token::Elif
                | Token::Fi
                | Token::Do
                | Token::Done
                | Token::Case
                | Token::Esac
                | Token::While
                | Token::Until
                | Token::For
                | Token::In
                | Token::LBrace
                | Token::RBrace
                | Token::Bang
                | Token::BashFunction
                | Token::BashSelect
                | Token::BashCoproc
        )
    }

    /// Returns `true` if this token is a redirection operator.
    pub fn is_redirect_op(&self) -> bool {
        matches!(
            self,
            Token::RedirectFromFile
                | Token::RedirectToFile
                | Token::Append
                | Token::HereDocOp
                | Token::HereDocStripOp
                | Token::RedirectFromFd
                | Token::RedirectToFd
                | Token::ReadWrite
                | Token::Clobber
                | Token::BashHereStringOp
                | Token::BashRedirectAllOp
                | Token::BashAppendAllOp
        )
    }

    /// Try to classify a word string as a reserved word token.
    pub fn reserved_word_from_str(s: &str) -> Option<Token> {
        match s {
            "if" => Some(Token::If),
            "then" => Some(Token::Then),
            "else" => Some(Token::Else),
            "elif" => Some(Token::Elif),
            "fi" => Some(Token::Fi),
            "do" => Some(Token::Do),
            "done" => Some(Token::Done),
            "case" => Some(Token::Case),
            "esac" => Some(Token::Esac),
            "while" => Some(Token::While),
            "until" => Some(Token::Until),
            "for" => Some(Token::For),
            "in" => Some(Token::In),
            "{" => Some(Token::LBrace),
            "}" => Some(Token::RBrace),
            "!" => Some(Token::Bang),
            _ => None,
        }
    }

    /// Human-readable name for use in error messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Token::Word(_) => "a word",
            Token::IoNumber(_) => "a file descriptor",
            Token::Newline => "newline",
            Token::If => "'if'",
            Token::Then => "'then'",
            Token::Else => "'else'",
            Token::Elif => "'elif'",
            Token::Fi => "'fi'",
            Token::Do => "'do'",
            Token::Done => "'done'",
            Token::Case => "'case'",
            Token::Esac => "'esac'",
            Token::While => "'while'",
            Token::Until => "'until'",
            Token::For => "'for'",
            Token::In => "'in'",
            Token::LBrace => "'{'",
            Token::RBrace => "'}'",
            Token::Bang => "'!'",
            Token::AndIf => "'&&'",
            Token::OrIf => "'||'",
            Token::CaseBreak => "';;'",
            Token::HereDocOp => "'<<'",
            Token::Append => "'>>'",
            Token::RedirectFromFd => "'<&'",
            Token::RedirectToFd => "'>&'",
            Token::ReadWrite => "'<>'",
            Token::Clobber => "'>|'",
            Token::HereDocStripOp => "'<<-'",
            Token::BashHereStringOp => "'<<<'",
            Token::BashRedirectAllOp => "'&>'",
            Token::BashAppendAllOp => "'&>>'",
            Token::BashDblLBracket => "'[['",
            Token::BashDblRBracket => "']]'",
            Token::BashCaseContinue => "';&'",
            Token::BashCaseFallThrough => "';;&'",
            Token::BashFunction => "'function'",
            Token::BashSelect => "'select'",
            Token::BashCoproc => "'coproc'",
            Token::BashPipeAmpersand => "'|&'",
            Token::Pipe => "'|'",
            Token::Semicolon => "';'",
            Token::Ampersand => "'&'",
            Token::RedirectFromFile => "'<'",
            Token::RedirectToFile => "'>'",
            Token::LParen => "'('",
            Token::RParen => "')'",
            Token::HereDocBody(_) => "here-document body",
            Token::Eof => "end of input",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_words_recognized() {
        assert!(Token::If.is_reserved_word());
        assert!(Token::Then.is_reserved_word());
        assert!(Token::Done.is_reserved_word());
        assert!(Token::Bang.is_reserved_word());
        assert!(!Token::Pipe.is_reserved_word());
        assert!(!Token::Word("if".into()).is_reserved_word());
    }

    #[test]
    fn redirect_ops_recognized() {
        assert!(Token::RedirectFromFile.is_redirect_op());
        assert!(Token::RedirectToFile.is_redirect_op());
        assert!(Token::Append.is_redirect_op());
        assert!(Token::Clobber.is_redirect_op());
        assert!(!Token::Pipe.is_redirect_op());
        assert!(!Token::Semicolon.is_redirect_op());
    }

    #[test]
    fn reserved_word_from_str_works() {
        assert_eq!(Token::reserved_word_from_str("if"), Some(Token::If));
        assert_eq!(Token::reserved_word_from_str("done"), Some(Token::Done));
        assert_eq!(Token::reserved_word_from_str("{"), Some(Token::LBrace));
        assert_eq!(Token::reserved_word_from_str("!"), Some(Token::Bang));
        assert_eq!(Token::reserved_word_from_str("echo"), None);
        assert_eq!(Token::reserved_word_from_str(""), None);
    }
}
