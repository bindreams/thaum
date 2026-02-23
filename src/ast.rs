use serde::{Deserialize, Serialize};

use crate::span::Span;

/// A newline-delimited group of statements.
///
/// In bash, each line is read, alias-expanded, parsed, and executed before the
/// next line is read.  Semicolons within a line do NOT create alias boundaries.
pub type Line = Vec<Statement>;

/// A complete parsed shell program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Program {
    pub lines: Vec<Line>,
    pub span: Span,
}

/// A statement: an expression with an execution mode.
///
/// Statements appear at list boundaries (program top-level, compound command
/// bodies) — the only places where `;` and `&` are valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    pub expression: Expression,
    pub mode: ExecutionMode,
    pub span: Span,
}

/// How a statement is executed.
///
/// `Sequential` vs `Terminated` matters for semantics: newline-separated
/// statements are distinct complete commands (e.g. `set -e` checks exit
/// status between them), while `;`-separated statements form a single
/// list where behavior may differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Newline-terminated or last in list. Each newline-separated statement
    /// is a separate complete command.
    Sequential,
    /// Explicitly terminated with `;`. Multiple `;`-separated statements
    /// form a single list, which can affect `set -e` behavior and other
    /// list-level semantics.
    Terminated,
    /// Run in background (`&`).
    Background,
}

/// An expression in the AST — the core command tree.
///
/// Binary operators (`And`, `Or`, `Pipe`) form left-associative trees.
/// Precedence from low to high: `&&`/`||`, `!`, `|`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expression {
    /// A simple command: name, arguments, assignments, redirections.
    Command(Command),
    /// A compound command (if/while/for/case/brace/subshell) with optional redirections.
    Compound {
        body: CompoundCommand,
        redirects: Vec<Redirect>,
    },
    /// A function definition: `name() compound_command`.
    FunctionDef(FunctionDef),
    /// `left && right`
    And {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// `left || right`
    Or {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// `left | right` or `left |& right` (Bash, pipes stderr too).
    Pipe {
        left: Box<Expression>,
        right: Box<Expression>,
        /// When true, stderr is also piped (`|&`, Bash only).
        stderr: bool,
    },
    /// `! expression` — negates the exit status.
    Not(Box<Expression>),
}

/// A simple command: optional assignments, arguments, and redirections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub assignments: Vec<Assignment>,
    /// First element is the command name, rest are arguments.
    pub arguments: Vec<Argument>,
    pub redirects: Vec<Redirect>,
    pub span: Span,
}

/// A single argument in a command's argument list.
///
/// Most arguments are composed `Word`s (one or more `Fragment`s concatenated).
/// Some (like process substitution) are standalone `Atom`s that always occupy
/// an entire argument slot by themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Argument {
    /// A composed word: one or more fragments concatenated.
    Word(Word),
    /// A standalone argument that cannot be part of a larger word.
    Atom(Atom),
}

impl Argument {
    /// Get the source span of this argument.
    pub fn span(&self) -> Span {
        match self {
            Argument::Word(w) => w.span,
            Argument::Atom(a) => match a {
                Atom::BashProcessSubstitution { span, .. } => *span,
            },
        }
    }

    /// If this argument resolves to a statically known string (no runtime
    /// expansion needed), return it. `Atom` variants always return `None`.
    pub fn try_to_static_string(&self) -> Option<String> {
        match self {
            Argument::Word(w) => w.try_to_static_string(),
            Argument::Atom(_) => None,
        }
    }
}

/// A standalone argument that always occupies one argument slot by itself.
///
/// Unlike `Fragment`s, atoms cannot be concatenated with other parts to form
/// a larger word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Atom {
    /// `<(cmd)` or `>(cmd)` — process substitution (Bash).
    BashProcessSubstitution {
        direction: ProcessDirection,
        body: Vec<Statement>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub name: String,
    /// Array subscript, if present: `name[index]=value`.
    pub index: Option<String>,
    pub value: AssignmentValue,
    pub span: Span,
}

/// The right-hand side of an assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentValue {
    /// A scalar value: `name=word`.
    Scalar(Word),
    /// An array literal: `name=(word1 word2 ...)` (Bash).
    BashArray(Vec<Word>),
}

impl AssignmentValue {
    /// Get the value as a scalar `Word`, panicking on `BashArray`.
    pub fn as_scalar(&self) -> &Word {
        match self {
            AssignmentValue::Scalar(w) => w,
            AssignmentValue::BashArray(_) => panic!("expected scalar assignment, got array"),
        }
    }
}

/// A function definition: `name () compound_command [redirects]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub body: Box<CompoundCommand>,
    pub redirects: Vec<Redirect>,
    pub span: Span,
}

/// Compound commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompoundCommand {
    BraceGroup {
        body: Vec<Line>,
        span: Span,
    },
    Subshell {
        body: Vec<Line>,
        span: Span,
    },
    ForClause {
        variable: String,
        words: Option<Vec<Word>>,
        body: Vec<Line>,
        span: Span,
    },
    CaseClause {
        word: Word,
        arms: Vec<CaseArm>,
        span: Span,
    },
    IfClause {
        condition: Vec<Line>,
        then_body: Vec<Line>,
        elifs: Vec<ElifClause>,
        else_body: Option<Vec<Line>>,
        span: Span,
    },
    WhileClause {
        condition: Vec<Line>,
        body: Vec<Line>,
        span: Span,
    },
    UntilClause {
        condition: Vec<Line>,
        body: Vec<Line>,
        span: Span,
    },
    // --- Bash extensions ---
    /// `[[ expression ]]` — extended test command (Bash).
    BashDoubleBracket {
        expression: BashTestExpr,
        span: Span,
    },
    /// `(( expression ))` — arithmetic command (Bash).
    BashArithmeticCommand {
        expression: ArithExpr,
        span: Span,
    },
    /// `select variable [in words...]; do body; done` — select loop (Bash).
    BashSelectClause {
        variable: String,
        words: Option<Vec<Word>>,
        body: Vec<Line>,
        span: Span,
    },
    /// `coproc [NAME] command` — coprocess (Bash).
    BashCoproc {
        name: Option<String>,
        body: Box<Expression>,
        span: Span,
    },
    /// `for ((init; cond; update)); do body; done` — C-style for loop (Bash).
    BashArithmeticFor {
        init: Option<ArithExpr>,
        condition: Option<ArithExpr>,
        update: Option<ArithExpr>,
        body: Vec<Line>,
        span: Span,
    },
}

/// How a case arm is terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseTerminator {
    /// `;;` — stop matching (POSIX).
    Break,
    /// `;;&` — test next pattern (Bash).
    BashFallThrough,
    /// `;&` — execute next arm without testing (Bash).
    BashContinue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: Vec<Line>,
    pub terminator: Option<CaseTerminator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElifClause {
    pub condition: Vec<Line>,
    pub body: Vec<Line>,
    pub span: Span,
}

/// A shell word — a sequence of fragments that are concatenated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Word {
    pub parts: Vec<Fragment>,
    pub span: Span,
}

impl Word {
    /// If every fragment in this word resolves statically, concatenate them
    /// and return the result. Returns `None` if any fragment requires runtime
    /// expansion (parameters, command substitutions, globs, etc.).
    pub fn try_to_static_string(&self) -> Option<String> {
        let mut result = String::new();
        for part in &self.parts {
            part.append_static_string(&mut result)?;
        }
        Some(result)
    }
}

/// A concatenable piece within a word.
///
/// Fragments can be combined with other fragments to form a single `Word`.
/// For example, `foo${bar}baz` is three fragments: `Literal`, `Parameter`,
/// `Literal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fragment {
    Literal(String),
    SingleQuoted(String),
    DoubleQuoted(Vec<Fragment>),
    Parameter(ParameterExpansion),
    CommandSubstitution(Vec<Statement>),
    ArithmeticExpansion(ArithExpr),
    Glob(GlobChar),
    TildePrefix(String),
    /// `$'...'` — ANSI-C quoting (Bash). Escapes stored literally, not interpreted.
    BashAnsiCQuoted(String),
    /// `$"..."` — locale translation quoting (Bash). Inner fragments undergo
    /// the same expansion as double quotes.
    BashLocaleQuoted(Vec<Fragment>),
    /// Extended glob pattern (Bash extglob): `?(pat)`, `*(pat)`, etc.
    BashExtGlob {
        kind: ExtGlobKind,
        pattern: String,
    },
    /// Brace expansion (Bash): `{a,b,c}` or `{1..5}`.
    BashBraceExpansion(BraceExpansionKind),
}

impl Fragment {
    /// If this fragment has a statically known string value (no runtime
    /// expansion needed), return it. Returns `None` for parameters,
    /// command substitutions, arithmetic, globs, tilde prefixes, locale
    /// quoting, extglobs, and brace expansions.
    pub fn try_to_static_string(&self) -> Option<String> {
        let mut result = String::new();
        self.append_static_string(&mut result)?;
        Some(result)
    }

    /// Append this fragment's static string value to `buf`, or return `None`
    /// if the fragment requires runtime expansion.
    fn append_static_string(&self, buf: &mut String) -> Option<()> {
        match self {
            Fragment::Literal(s) | Fragment::SingleQuoted(s) | Fragment::BashAnsiCQuoted(s) => {
                buf.push_str(s);
            }
            Fragment::DoubleQuoted(parts) => {
                for part in parts {
                    part.append_static_string(buf)?;
                }
            }
            Fragment::Parameter(_)
            | Fragment::CommandSubstitution(_)
            | Fragment::ArithmeticExpansion(_)
            | Fragment::Glob(_)
            | Fragment::TildePrefix(_)
            | Fragment::BashLocaleQuoted(_)
            | Fragment::BashExtGlob { .. }
            | Fragment::BashBraceExpansion(_) => return None,
        }
        Some(())
    }
}

/// Kind of brace expansion (Bash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BraceExpansionKind {
    /// `{word,word,...}` — comma-separated alternatives.
    List(Vec<Vec<Fragment>>),
    /// `{start..end[..step]}` — numeric or character sequence.
    Sequence {
        start: String,
        end: String,
        step: Option<String>,
    },
}

/// Kind of extended glob pattern (Bash extglob).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtGlobKind {
    /// `?(pattern)` — matches zero or one occurrence.
    ZeroOrOne,
    /// `*(pattern)` — matches zero or more occurrences.
    ZeroOrMore,
    /// `+(pattern)` — matches one or more occurrences.
    OneOrMore,
    /// `@(pattern)` — matches exactly one occurrence.
    ExactlyOne,
    /// `!(pattern)` — matches anything except the pattern.
    Not,
}

/// Expression inside `[[ ]]` (Bash extended test).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BashTestExpr {
    /// `-op arg` (unary file/string test).
    Unary { op: UnaryTestOp, arg: Word },
    /// `arg op arg` (binary comparison).
    Binary {
        left: Word,
        op: BinaryTestOp,
        right: Word,
    },
    /// `expr && expr`.
    And {
        left: Box<BashTestExpr>,
        right: Box<BashTestExpr>,
    },
    /// `expr || expr`.
    Or {
        left: Box<BashTestExpr>,
        right: Box<BashTestExpr>,
    },
    /// `! expr`.
    Not(Box<BashTestExpr>),
    /// `( expr )` — grouped sub-expression.
    Group(Box<BashTestExpr>),
    /// Bare word — implicit `-n` test (true if non-empty string).
    Word(Word),
}

/// Unary test operator inside `[[ ]]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryTestOp {
    // File existence/type
    FileExists,         // -e
    FileIsRegular,      // -f
    FileIsDirectory,    // -d
    FileIsSymlink,      // -L, -h
    FileIsBlockDev,     // -b
    FileIsCharDev,      // -c
    FileIsPipe,         // -p
    FileIsSocket,       // -S (uppercase)
    FileHasSize,        // -s
    FileDescriptorOpen, // -t
    // File permissions
    FileIsReadable,        // -r
    FileIsWritable,        // -w
    FileIsExecutable,      // -x
    FileIsSetuid,          // -u
    FileIsSetgid,          // -g
    FileIsSticky,          // -k
    FileIsOwnedByUser,     // -O
    FileIsOwnedByGroup,    // -G
    FileModifiedSinceRead, // -N
    // String tests
    StringIsEmpty,    // -z
    StringIsNonEmpty, // -n
    // Variable tests (Bash 4.2+)
    VariableIsSet,     // -v
    VariableIsNameRef, // -R
}

/// Binary test operator inside `[[ ]]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryTestOp {
    // String comparison
    StringEquals,      // == or =
    StringNotEquals,   // !=
    StringLessThan,    // <
    StringGreaterThan, // >
    RegexMatch,        // =~
    // Integer comparison
    IntEq, // -eq
    IntNe, // -ne
    IntLt, // -lt
    IntLe, // -le
    IntGt, // -gt
    IntGe, // -ge
    // File comparison
    FileNewerThan,  // -nt
    FileOlderThan,  // -ot
    FileSameDevice, // -ef
}

/// Arithmetic expression (Bash). Used by `(( ))` command, `$(( ))` expansion,
/// and `for (( ; ; ))` loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithExpr {
    /// Integer literal: `42`, `0x1F`.
    Number(i64),
    /// Variable reference: `x`, `arr[i]`.
    Variable(String),
    /// Binary operation: `a + b`.
    Binary {
        left: Box<ArithExpr>,
        op: ArithBinaryOp,
        right: Box<ArithExpr>,
    },
    /// Unary prefix: `-x`, `!x`, `~x`, `++x`, `--x`.
    UnaryPrefix {
        op: ArithUnaryOp,
        operand: Box<ArithExpr>,
    },
    /// Unary postfix: `x++`, `x--`.
    UnaryPostfix {
        operand: Box<ArithExpr>,
        op: ArithUnaryOp,
    },
    /// Ternary: `cond ? then : else`.
    Ternary {
        condition: Box<ArithExpr>,
        then_expr: Box<ArithExpr>,
        else_expr: Box<ArithExpr>,
    },
    /// Assignment: `x = expr`, `x += expr`.
    Assignment {
        target: String,
        op: ArithAssignOp,
        value: Box<ArithExpr>,
    },
    /// Grouped: `( expr )`.
    Group(Box<ArithExpr>),
    /// Comma expression: `expr, expr` (evaluate both, return right).
    Comma {
        left: Box<ArithExpr>,
        right: Box<ArithExpr>,
    },
}

/// Arithmetic binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,        // **
    ShiftLeft,  // <<
    ShiftRight, // >>
    BitAnd,     // &
    BitOr,      // |
    BitXor,     // ^
    LogAnd,     // &&
    LogOr,      // ||
    Eq,         // ==
    Ne,         // !=
    Lt,         // <
    Le,         // <=
    Gt,         // >
    Ge,         // >=
}

/// Arithmetic unary operator (prefix or postfix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithUnaryOp {
    Negate,    // - (prefix)
    Plus,      // + (prefix, no-op)
    LogNot,    // !
    BitNot,    // ~
    Increment, // ++
    Decrement, // --
}

/// Arithmetic assignment operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithAssignOp {
    Assign,           // =
    AddAssign,        // +=
    SubAssign,        // -=
    MulAssign,        // *=
    DivAssign,        // /=
    ModAssign,        // %=
    ShiftLeftAssign,  // <<=
    ShiftRightAssign, // >>=
    BitAndAssign,     // &=
    BitOrAssign,      // |=
    BitXorAssign,     // ^=
}

/// Direction of a Bash process substitution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessDirection {
    /// `<(cmd)` — read from command's stdout.
    In,
    /// `>(cmd)` — write to command's stdin.
    Out,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterExpansion {
    Simple(String),
    Complex {
        name: String,
        operator: Option<ParamOp>,
        argument: Option<Box<Word>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamOp {
    Default,
    DefaultAssign,
    Error,
    Alternative,
    Length,
    TrimSmallSuffix,
    TrimLargeSuffix,
    TrimSmallPrefix,
    TrimLargePrefix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlobChar {
    Star,
    Question,
    BracketOpen,
}

/// An I/O redirection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redirect {
    pub fd: Option<i32>,
    pub kind: RedirectKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedirectKind {
    Input(Word),
    Output(Word),
    Append(Word),
    Clobber(Word),
    ReadWrite(Word),
    DupInput(Word),
    DupOutput(Word),
    HereDoc {
        delimiter: String,
        body: String,
        strip_tabs: bool,
        quoted: bool,
    },
    // --- Bash extensions ---
    /// `<<<` here-string (Bash).
    BashHereString(Word),
    /// `&>` redirect stdout+stderr (Bash).
    BashOutputAll(Word),
    /// `&>>` append stdout+stderr (Bash).
    BashAppendAll(Word),
}

#[cfg(test)]
#[path = "ast_tests.rs"]
mod tests;
