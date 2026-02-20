use crate::ast::BinaryTestOp;
use crate::span::Span;

/// A token with its source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// Glob metacharacter kind for the `Glob` token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobKind {
    Star,
    Question,
    BracketOpen,
}

/// Extended glob prefix kind for the `BashExtGlob` token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtGlobTokenKind {
    /// `?(`
    ZeroOrOne,
    /// `*(`
    ZeroOrMore,
    /// `+(`
    OneOrMore,
    /// `@(`
    ExactlyOne,
    /// `!(`
    Not,
}

/// All token types recognized by the shell lexer.
#[derive(Debug, Clone, PartialEq, Eq, strum::IntoStaticStr)]
pub enum Token {
    // === Fragment tokens ===
    /// Unquoted literal text. Carries RAW characters including backslash escapes.
    /// De-escaping happens during AST construction, not here.
    Literal(String),
    /// Content between `'...'` (without the quote characters).
    SingleQuoted(String),
    /// Raw content between `"..."` (without outer quotes).
    /// The parser invokes an inner lexer in double-quote mode on this content.
    DoubleQuoted(String),
    /// `$VAR`, `$1`, `$@`, etc. — the name/char after `$`.
    SimpleParam(String),
    /// Raw content of `${...}` (without the `${` and `}`).
    /// Internal structure (name, operator, argument) parsed by a helper.
    BraceParam(String),
    /// Raw content of `$(...)` (without the `$(` and `)`).
    /// Recursive parser invocation produces `Vec<Statement>`.
    CommandSub(String),
    /// Raw content of `` `...` `` (without the backticks).
    BacktickSub(String),
    /// Raw content of `$((...))` (without the `$((` and `))`).
    ArithSub(String),
    /// Glob metacharacter: `*`, `?`, or `[`.
    Glob(GlobKind),
    /// `~user` at word start. String is the user part (empty for bare `~`).
    TildePrefix(String),
    /// `$'...'` content (without `$'` and `'`). Bash only.
    BashAnsiCQuoted(String),
    /// `$"..."` raw content (without `$"` and `"`). Bash only.
    /// The parser invokes an inner lexer in double-quote mode on this content.
    BashLocaleQuoted(String),
    /// `?(pat)`, `*(pat)`, `+(pat)`, `@(pat)`, `!(pat)`. Bash only.
    BashExtGlob {
        kind: ExtGlobTokenKind,
        pattern: String,
    },
    /// `<(...)` or `>(...)` — process substitution (Bash).
    BashProcessSub {
        /// `'<'` for input, `'>'` for output.
        direction: char,
        /// Raw content between the parentheses.
        content: String,
    },
    /// Unquoted whitespace between words (word boundary marker).
    Blank,

    // === Other value-carrying tokens ===
    /// An IO_NUMBER: a digit sequence immediately preceding `<` or `>`.
    IoNumber(i32),

    // === Newline (semantically significant in shell) ===
    Newline,

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

    // === Bash extension operators ===
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
    /// Returns `true` if this is a fragment token (part of a word).
    pub fn is_fragment(&self) -> bool {
        matches!(
            self,
            Token::Literal(_)
                | Token::SingleQuoted(_)
                | Token::DoubleQuoted(_)
                | Token::SimpleParam(_)
                | Token::BraceParam(_)
                | Token::CommandSub(_)
                | Token::BacktickSub(_)
                | Token::ArithSub(_)
                | Token::Glob(_)
                | Token::TildePrefix(_)
                | Token::BashAnsiCQuoted(_)
                | Token::BashLocaleQuoted(_)
                | Token::BashExtGlob { .. }
                | Token::BashProcessSub { .. }
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

    /// Token variant name for structured output (e.g. the `lex` CLI subcommand).
    ///
    /// Derived via `strum::IntoStaticStr` — returns the enum variant name
    /// as a `&'static str` (e.g. `"Literal"`, `"Pipe"`, `"AndIf"`).
    pub fn token_name(&self) -> &'static str {
        self.into()
    }

    /// Human-readable name for use in error messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Token::Literal(_) => "a word",
            Token::SingleQuoted(_) => "a word",
            Token::DoubleQuoted(_) => "a word",
            Token::SimpleParam(_) => "a word",
            Token::BraceParam(_) => "a word",
            Token::CommandSub(_) => "a word",
            Token::BacktickSub(_) => "a word",
            Token::ArithSub(_) => "a word",
            Token::Glob(_) => "a word",
            Token::TildePrefix(_) => "a word",
            Token::BashAnsiCQuoted(_) => "a word",
            Token::BashLocaleQuoted(_) => "a word",
            Token::BashExtGlob { .. } => "a word",
            Token::BashProcessSub { .. } => "a word",
            Token::Blank => "blank",
            Token::IoNumber(_) => "a file descriptor",
            Token::Newline => "newline",
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

    // ================================================================
    // Grammar-level queries
    //
    // These are pure functions on token values. The caller is responsible
    // for peeking the token(s) from the lexer — these methods never
    // interact with the lexer or its buffer.
    // ================================================================

    /// Can a redirect start with this token? (redirect operator or IO number)
    pub fn is_redirect_start(&self) -> bool {
        self.is_redirect_op() || matches!(self, Token::IoNumber(_))
    }

    /// Is this a lone Literal matching `expected` (i.e. a keyword)?
    /// `next` is the following token — a keyword is only recognized when
    /// it is not glued to adjacent fragments.
    pub fn is_keyword(&self, next: &Token, expected: &str) -> bool {
        matches!(self, Token::Literal(w) if w == expected) && !next.is_fragment()
    }

    /// Can a simple command begin with this token?
    /// `next` is the following token — needed to distinguish closing
    /// keywords (which cannot start commands) from identically-named
    /// literals that are glued to fragments (which can).
    pub fn can_start_command(&self, next: &Token) -> bool {
        match self {
            Token::Literal(w) if Self::is_closing_keyword(w) => next.is_fragment(),
            Token::Literal(_) => true,
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
            _ if self.is_fragment() => true,
            _ => false,
        }
    }

    /// Does this token start a compound command?
    /// `next` is the following token (keyword isolation check).
    /// `select_enabled` controls whether `select` is recognized.
    pub fn is_compound_start(&self, next: &Token, select_enabled: bool) -> bool {
        match self {
            Token::Literal(w) if Self::is_compound_keyword(w, select_enabled) => {
                !next.is_fragment()
            }
            Token::LParen | Token::BashDblLBracket => true,
            _ => false,
        }
    }

    /// Map this token to a binary test operator (for `[[ ]]` expressions).
    pub fn as_binary_test_op(&self) -> Option<BinaryTestOp> {
        match self {
            Token::Literal(s) => Self::word_as_binary_test_op(s),
            Token::RedirectFromFile => Some(BinaryTestOp::StringLessThan),
            Token::RedirectToFile => Some(BinaryTestOp::StringGreaterThan),
            _ => None,
        }
    }

    /// Is this word a closing reserved keyword that cannot start a command?
    pub fn is_closing_keyword(w: &str) -> bool {
        matches!(
            w,
            "then" | "else" | "elif" | "fi" | "do" | "done" | "esac" | "}" | "in"
        )
    }

    /// Is this word a compound-command keyword?
    pub fn is_compound_keyword(w: &str, select_enabled: bool) -> bool {
        matches!(w, "if" | "while" | "until" | "for" | "case" | "{")
            || (select_enabled && w == "select")
    }

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
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
