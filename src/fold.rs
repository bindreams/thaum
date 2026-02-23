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
    fn fold_program(&mut self, program: Program) -> Program {
        fold_program(self, program)
    }

    fn fold_statement(&mut self, statement: Statement) -> Statement {
        fold_statement(self, statement)
    }

    fn fold_expression(&mut self, expression: Expression) -> Expression {
        fold_expression(self, expression)
    }

    fn fold_command(&mut self, command: Command) -> Command {
        fold_command(self, command)
    }

    fn fold_compound_command(&mut self, compound: CompoundCommand) -> CompoundCommand {
        fold_compound_command(self, compound)
    }

    fn fold_function_def(&mut self, function_def: FunctionDef) -> FunctionDef {
        fold_function_def(self, function_def)
    }

    fn fold_redirect(&mut self, redirect: Redirect) -> Redirect {
        fold_redirect(self, redirect)
    }

    fn fold_assignment(&mut self, assignment: Assignment) -> Assignment {
        fold_assignment(self, assignment)
    }

    fn fold_case_arm(&mut self, arm: CaseArm) -> CaseArm {
        fold_case_arm(self, arm)
    }

    fn fold_elif_clause(&mut self, elif: ElifClause) -> ElifClause {
        fold_elif_clause(self, elif)
    }

    fn fold_argument(&mut self, argument: Argument) -> Argument {
        fold_argument(self, argument)
    }

    fn fold_word(&mut self, word: Word) -> Word {
        // Leaf by default. Word-level rewriting is opt-in.
        word
    }
}

// ---------------------------------------------------------------------------
// fold_* free functions
// ---------------------------------------------------------------------------

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

pub fn fold_program<F: Fold + ?Sized>(f: &mut F, program: Program) -> Program {
    Program {
        lines: fold_lines(f, program.lines),
        span: program.span,
    }
}

pub fn fold_statement<F: Fold + ?Sized>(f: &mut F, stmt: Statement) -> Statement {
    Statement {
        expression: f.fold_expression(stmt.expression),
        mode: stmt.mode,
        span: stmt.span,
    }
}

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
        Expression::Pipe {
            left,
            right,
            stderr,
        } => Expression::Pipe {
            left: Box::new(f.fold_expression(*left)),
            right: Box::new(f.fold_expression(*right)),
            stderr,
        },
        Expression::Not(inner) => Expression::Not(Box::new(f.fold_expression(*inner))),
    }
}

pub fn fold_command<F: Fold + ?Sized>(f: &mut F, cmd: Command) -> Command {
    Command {
        assignments: cmd
            .assignments
            .into_iter()
            .map(|a| f.fold_assignment(a))
            .collect(),
        arguments: cmd
            .arguments
            .into_iter()
            .map(|a| f.fold_argument(a))
            .collect(),
        redirects: fold_redirects(f, cmd.redirects),
        span: cmd.span,
    }
}

pub fn fold_compound_command<F: Fold + ?Sized>(
    f: &mut F,
    compound: CompoundCommand,
) -> CompoundCommand {
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
        CompoundCommand::WhileClause {
            condition,
            body,
            span,
        } => CompoundCommand::WhileClause {
            condition: fold_lines(f, condition),
            body: fold_lines(f, body),
            span,
        },
        CompoundCommand::UntilClause {
            condition,
            body,
            span,
        } => CompoundCommand::UntilClause {
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

pub fn fold_function_def<F: Fold + ?Sized>(f: &mut F, fndef: FunctionDef) -> FunctionDef {
    FunctionDef {
        name: fndef.name,
        body: Box::new(f.fold_compound_command(*fndef.body)),
        redirects: fold_redirects(f, fndef.redirects),
        span: fndef.span,
    }
}

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

pub fn fold_case_arm<F: Fold + ?Sized>(f: &mut F, arm: CaseArm) -> CaseArm {
    CaseArm {
        patterns: fold_words(f, arm.patterns),
        body: fold_lines(f, arm.body),
        terminator: arm.terminator,
        span: arm.span,
    }
}

pub fn fold_elif_clause<F: Fold + ?Sized>(f: &mut F, elif: ElifClause) -> ElifClause {
    ElifClause {
        condition: fold_lines(f, elif.condition),
        body: fold_lines(f, elif.body),
        span: elif.span,
    }
}

pub fn fold_argument<F: Fold + ?Sized>(f: &mut F, argument: Argument) -> Argument {
    match argument {
        Argument::Word(w) => Argument::Word(f.fold_word(w)),
        atom @ Argument::Atom(_) => atom,
    }
}

#[cfg(test)]
#[path = "fold_tests.rs"]
mod tests;
