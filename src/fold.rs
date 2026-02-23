//! Ownership-based AST rewriting (fold / catamorphism).
//!
//! Each `fold_*` trait method takes a node **by value** and returns the
//! (possibly modified) replacement. The default implementations delegate to
//! the corresponding free `fold_*` function, which recursively folds child
//! nodes and reconstructs the parent.
//!
//! Override a method to intercept a node type; call the free function inside
//! your override to continue descending into children.
//!
//! Like [`Visit`](crate::visit::Visit), the fold does **not** automatically
//! descend into word-level nesting such as
//! [`Fragment::CommandSubstitution`](crate::ast::Fragment::CommandSubstitution).
//! Override [`Fold::fold_word`] to enter those manually.
//!
//! # Example
//!
//! ```
//! use thaum::ast::*;
//! use thaum::fold::{self, Fold};
//!
//! /// Uppercases every literal fragment in the AST.
//! struct Uppercaser;
//!
//! impl Fold for Uppercaser {
//!     fn fold_word(&mut self, mut word: Word) -> Word {
//!         for part in &mut word.parts {
//!             if let Fragment::Literal(s) = part {
//!                 *s = s.to_uppercase();
//!             }
//!         }
//!         word
//!     }
//! }
//!
//! let prog = thaum::parse("echo hello").unwrap();
//! let prog = Uppercaser.fold_program(prog);
//! let cmd = match &prog.lines[0][0].expression {
//!     Expression::Command(c) => c,
//!     _ => panic!(),
//! };
//! assert_eq!(cmd.arguments[0].try_to_static_string(), Some("ECHO".into()));
//! assert_eq!(cmd.arguments[1].try_to_static_string(), Some("HELLO".into()));
//! ```

use crate::ast::*;

/// Trait for ownership-based AST rewriting.
///
/// See the [module-level documentation](self) for usage.
pub trait Fold {
    /// Rewrite a complete program. Default: folds each line.
    fn fold_program(&mut self, program: Program) -> Program {
        fold_program(self, program)
    }

    /// Rewrite a statement. Default: folds the inner expression.
    fn fold_statement(&mut self, statement: Statement) -> Statement {
        fold_statement(self, statement)
    }

    /// Rewrite an expression tree. Default: recurses into children.
    fn fold_expression(&mut self, expression: Expression) -> Expression {
        fold_expression(self, expression)
    }

    /// Rewrite a simple command. Default: folds assignments, arguments, redirects.
    fn fold_command(&mut self, command: Command) -> Command {
        fold_command(self, command)
    }

    /// Rewrite a compound command. Default: folds body and sub-structures.
    fn fold_compound_command(&mut self, compound: CompoundCommand) -> CompoundCommand {
        fold_compound_command(self, compound)
    }

    /// Rewrite a function definition. Default: folds body and redirects.
    fn fold_function_def(&mut self, function_def: FunctionDef) -> FunctionDef {
        fold_function_def(self, function_def)
    }

    /// Rewrite a redirect. Default: folds the target word.
    fn fold_redirect(&mut self, redirect: Redirect) -> Redirect {
        fold_redirect(self, redirect)
    }

    /// Rewrite an assignment. Default: folds the value word(s).
    fn fold_assignment(&mut self, assignment: Assignment) -> Assignment {
        fold_assignment(self, assignment)
    }

    /// Rewrite a case arm. Default: folds patterns and body.
    fn fold_case_arm(&mut self, arm: CaseArm) -> CaseArm {
        fold_case_arm(self, arm)
    }

    /// Rewrite an elif clause. Default: folds condition and body.
    fn fold_elif_clause(&mut self, elif: ElifClause) -> ElifClause {
        fold_elif_clause(self, elif)
    }

    /// Rewrite an argument. Default: folds the inner word (atoms pass through).
    fn fold_argument(&mut self, argument: Argument) -> Argument {
        fold_argument(self, argument)
    }

    /// Rewrite a word. Default is a **no-op** (leaf node). Override to enter
    /// word-level nesting (fragments, command substitutions, etc.).
    fn fold_word(&mut self, word: Word) -> Word {
        // Leaf by default. Word-level rewriting is opt-in.
        word
    }
}

// fold_* free functions ===============================================================================================

fn fold_stmts<F: Fold + ?Sized>(f: &mut F, stmts: Vec<Statement>) -> Vec<Statement> {
    stmts.into_iter().map(|s| f.fold_statement(s)).collect()
}

fn fold_lines<F: Fold + ?Sized>(f: &mut F, lines: Vec<Line>) -> Vec<Line> {
    lines.into_iter().map(|line| fold_stmts(f, line)).collect()
}

fn fold_words<F: Fold + ?Sized>(f: &mut F, words: Vec<Word>) -> Vec<Word> {
    words.into_iter().map(|w| f.fold_word(w)).collect()
}

fn fold_redirects<F: Fold + ?Sized>(f: &mut F, redirects: Vec<Redirect>) -> Vec<Redirect> {
    redirects.into_iter().map(|r| f.fold_redirect(r)).collect()
}

/// Recursively fold all lines in a program. Call from [`Fold::fold_program`] overrides.
pub fn fold_program<F: Fold + ?Sized>(f: &mut F, program: Program) -> Program {
    Program {
        lines: fold_lines(f, program.lines),
        span: program.span,
    }
}

/// Fold the expression inside a statement. Call from [`Fold::fold_statement`] overrides.
pub fn fold_statement<F: Fold + ?Sized>(f: &mut F, stmt: Statement) -> Statement {
    Statement {
        expression: f.fold_expression(stmt.expression),
        mode: stmt.mode,
        span: stmt.span,
    }
}

/// Recursively fold child expressions. Call from [`Fold::fold_expression`] overrides.
pub fn fold_expression<F: Fold + ?Sized>(f: &mut F, expr: Expression) -> Expression {
    match expr {
        Expression::Command(cmd) => Expression::Command(f.fold_command(cmd)),
        Expression::Compound { body, redirects } => Expression::Compound {
            body: f.fold_compound_command(body),
            redirects: fold_redirects(f, redirects),
        },
        Expression::FunctionDef(fndef) => Expression::FunctionDef(f.fold_function_def(fndef)),
        Expression::And { left, right } => Expression::And {
            left: Box::new(f.fold_expression(*left)),
            right: Box::new(f.fold_expression(*right)),
        },
        Expression::Or { left, right } => Expression::Or {
            left: Box::new(f.fold_expression(*left)),
            right: Box::new(f.fold_expression(*right)),
        },
        Expression::Pipe { left, right, stderr } => Expression::Pipe {
            left: Box::new(f.fold_expression(*left)),
            right: Box::new(f.fold_expression(*right)),
            stderr,
        },
        Expression::Not(inner) => Expression::Not(Box::new(f.fold_expression(*inner))),
    }
}

/// Fold assignments, arguments, and redirects. Call from [`Fold::fold_command`] overrides.
pub fn fold_command<F: Fold + ?Sized>(f: &mut F, cmd: Command) -> Command {
    Command {
        assignments: cmd.assignments.into_iter().map(|a| f.fold_assignment(a)).collect(),
        arguments: cmd.arguments.into_iter().map(|a| f.fold_argument(a)).collect(),
        redirects: fold_redirects(f, cmd.redirects),
        span: cmd.span,
    }
}

/// Fold all body/sub-structure in a compound command. Call from [`Fold::fold_compound_command`] overrides.
pub fn fold_compound_command<F: Fold + ?Sized>(f: &mut F, compound: CompoundCommand) -> CompoundCommand {
    match compound {
        CompoundCommand::BraceGroup { body, span } => CompoundCommand::BraceGroup {
            body: fold_lines(f, body),
            span,
        },
        CompoundCommand::Subshell { body, span } => CompoundCommand::Subshell {
            body: fold_lines(f, body),
            span,
        },
        CompoundCommand::ForClause {
            variable,
            words,
            body,
            span,
        } => CompoundCommand::ForClause {
            variable,
            words: words.map(|w| fold_words(f, w)),
            body: fold_lines(f, body),
            span,
        },
        CompoundCommand::CaseClause { word, arms, span } => CompoundCommand::CaseClause {
            word: f.fold_word(word),
            arms: arms.into_iter().map(|a| f.fold_case_arm(a)).collect(),
            span,
        },
        CompoundCommand::IfClause {
            condition,
            then_body,
            elifs,
            else_body,
            span,
        } => CompoundCommand::IfClause {
            condition: fold_lines(f, condition),
            then_body: fold_lines(f, then_body),
            elifs: elifs.into_iter().map(|e| f.fold_elif_clause(e)).collect(),
            else_body: else_body.map(|b| fold_lines(f, b)),
            span,
        },
        CompoundCommand::WhileClause { condition, body, span } => CompoundCommand::WhileClause {
            condition: fold_lines(f, condition),
            body: fold_lines(f, body),
            span,
        },
        CompoundCommand::UntilClause { condition, body, span } => CompoundCommand::UntilClause {
            condition: fold_lines(f, condition),
            body: fold_lines(f, body),
            span,
        },
        CompoundCommand::BashDoubleBracket { expression, span } => {
            CompoundCommand::BashDoubleBracket { expression, span }
        }
        CompoundCommand::BashArithmeticCommand { expression, span } => {
            CompoundCommand::BashArithmeticCommand { expression, span }
        }
        CompoundCommand::BashSelectClause {
            variable,
            words,
            body,
            span,
        } => CompoundCommand::BashSelectClause {
            variable,
            words: words.map(|w| fold_words(f, w)),
            body: fold_lines(f, body),
            span,
        },
        CompoundCommand::BashCoproc { name, body, span } => CompoundCommand::BashCoproc {
            name,
            body: Box::new(f.fold_expression(*body)),
            span,
        },
        CompoundCommand::BashArithmeticFor {
            init,
            condition,
            update,
            body,
            span,
        } => CompoundCommand::BashArithmeticFor {
            init,
            condition,
            update,
            body: fold_lines(f, body),
            span,
        },
    }
}

/// Fold body and redirects of a function definition. Call from [`Fold::fold_function_def`] overrides.
pub fn fold_function_def<F: Fold + ?Sized>(f: &mut F, fndef: FunctionDef) -> FunctionDef {
    FunctionDef {
        name: fndef.name,
        body: Box::new(f.fold_compound_command(*fndef.body)),
        redirects: fold_redirects(f, fndef.redirects),
        span: fndef.span,
    }
}

/// Fold the target word inside a redirect. Call from [`Fold::fold_redirect`] overrides.
pub fn fold_redirect<F: Fold + ?Sized>(f: &mut F, redirect: Redirect) -> Redirect {
    let kind = match redirect.kind {
        RedirectKind::Input(w) => RedirectKind::Input(f.fold_word(w)),
        RedirectKind::Output(w) => RedirectKind::Output(f.fold_word(w)),
        RedirectKind::Append(w) => RedirectKind::Append(f.fold_word(w)),
        RedirectKind::Clobber(w) => RedirectKind::Clobber(f.fold_word(w)),
        RedirectKind::ReadWrite(w) => RedirectKind::ReadWrite(f.fold_word(w)),
        RedirectKind::DupInput(w) => RedirectKind::DupInput(f.fold_word(w)),
        RedirectKind::DupOutput(w) => RedirectKind::DupOutput(f.fold_word(w)),
        RedirectKind::BashHereString(w) => RedirectKind::BashHereString(f.fold_word(w)),
        RedirectKind::BashOutputAll(w) => RedirectKind::BashOutputAll(f.fold_word(w)),
        RedirectKind::BashAppendAll(w) => RedirectKind::BashAppendAll(f.fold_word(w)),
        heredoc @ RedirectKind::HereDoc { .. } => heredoc,
    };
    Redirect {
        fd: redirect.fd,
        kind,
        span: redirect.span,
    }
}

/// Fold the value word(s) in an assignment. Call from [`Fold::fold_assignment`] overrides.
pub fn fold_assignment<F: Fold + ?Sized>(f: &mut F, assignment: Assignment) -> Assignment {
    let value = match assignment.value {
        AssignmentValue::Scalar(w) => AssignmentValue::Scalar(f.fold_word(w)),
        AssignmentValue::BashArray(words) => AssignmentValue::BashArray(fold_words(f, words)),
    };
    Assignment {
        name: assignment.name,
        index: assignment.index,
        value,
        span: assignment.span,
    }
}

/// Fold patterns and body in a case arm. Call from [`Fold::fold_case_arm`] overrides.
pub fn fold_case_arm<F: Fold + ?Sized>(f: &mut F, arm: CaseArm) -> CaseArm {
    CaseArm {
        patterns: fold_words(f, arm.patterns),
        body: fold_lines(f, arm.body),
        terminator: arm.terminator,
        span: arm.span,
    }
}

/// Fold condition and body in an elif clause. Call from [`Fold::fold_elif_clause`] overrides.
pub fn fold_elif_clause<F: Fold + ?Sized>(f: &mut F, elif: ElifClause) -> ElifClause {
    ElifClause {
        condition: fold_lines(f, elif.condition),
        body: fold_lines(f, elif.body),
        span: elif.span,
    }
}

/// Fold the word inside an argument (atoms pass through unchanged). Call from [`Fold::fold_argument`] overrides.
pub fn fold_argument<F: Fold + ?Sized>(f: &mut F, argument: Argument) -> Argument {
    match argument {
        Argument::Word(w) => Argument::Word(f.fold_word(w)),
        atom @ Argument::Atom(_) => atom,
    }
}

#[cfg(test)]
#[path = "fold_tests.rs"]
mod tests;
