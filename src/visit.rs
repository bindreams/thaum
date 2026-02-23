//! Immutable AST visitor.
//!
//! Each `visit_*` trait method has a default that delegates to the
//! corresponding free `walk_*` function, which visits child nodes.
//! Override a method to intercept a node; call the `walk_*` function
//! inside your override to continue descending.
//!
//! The visitor covers the statement / expression / command tree. It does
//! **not** automatically descend into word-level nesting such as
//! [`Fragment::CommandSubstitution`] or [`Atom::BashProcessSubstitution`].
//! Override [`Visit::visit_word`] or [`Visit::visit_argument`] to enter
//! those manually.
//!
//! # Example
//!
//! ```
//! use thaum::ast::*;
//! use thaum::visit::{self, Visit};
//!
//! struct CommandNames(Vec<String>);
//!
//! impl<'ast> Visit<'ast> for CommandNames {
//!     fn visit_command(&mut self, cmd: &'ast Command) {
//!         if let Some(name) = cmd.arguments.first().and_then(|a| a.try_to_static_string()) {
//!             self.0.push(name);
//!         }
//!         visit::walk_command(self, cmd);
//!     }
//! }
//!
//! let prog = thaum::parse("echo hi; ls | grep foo").unwrap();
//! let mut v = CommandNames(vec![]);
//! v.visit_program(&prog);
//! assert_eq!(v.0, vec!["echo", "ls", "grep"]);
//! ```

use crate::ast::*;

/// Trait for immutable AST traversal.
///
/// See the [module-level documentation](self) for usage.
pub trait Visit<'ast> {
    /// Visit a complete program. Default: walks all lines.
    fn visit_program(&mut self, program: &'ast Program) {
        walk_program(self, program);
    }

    /// Visit a statement. Default: visits the inner expression.
    fn visit_statement(&mut self, statement: &'ast Statement) {
        walk_statement(self, statement);
    }

    /// Visit an expression tree. Default: recurses into children.
    fn visit_expression(&mut self, expression: &'ast Expression) {
        walk_expression(self, expression);
    }

    /// Visit a simple command. Default: visits assignments, arguments, redirects.
    fn visit_command(&mut self, command: &'ast Command) {
        walk_command(self, command);
    }

    /// Visit a compound command. Default: visits body and sub-structures.
    fn visit_compound_command(&mut self, compound: &'ast CompoundCommand) {
        walk_compound_command(self, compound);
    }

    /// Visit a function definition. Default: visits body and redirects.
    fn visit_function_def(&mut self, function_def: &'ast FunctionDef) {
        walk_function_def(self, function_def);
    }

    /// Visit a redirect. Default: visits the target word.
    fn visit_redirect(&mut self, redirect: &'ast Redirect) {
        walk_redirect(self, redirect);
    }

    /// Visit an assignment. Default: visits the value word(s).
    fn visit_assignment(&mut self, assignment: &'ast Assignment) {
        walk_assignment(self, assignment);
    }

    /// Visit a case arm. Default: visits patterns and body.
    fn visit_case_arm(&mut self, arm: &'ast CaseArm) {
        walk_case_arm(self, arm);
    }

    /// Visit an elif clause. Default: visits condition and body.
    fn visit_elif_clause(&mut self, elif: &'ast ElifClause) {
        walk_elif_clause(self, elif);
    }

    /// Visit an argument. Default: visits the inner word (atoms are not entered).
    fn visit_argument(&mut self, argument: &'ast Argument) {
        walk_argument(self, argument);
    }

    /// Visit a word. Default is a **no-op** (leaf node). Override to enter
    /// word-level nesting (fragments, command substitutions, etc.).
    fn visit_word(&mut self, _word: &'ast Word) {
        // Leaf by default. Word-level traversal is opt-in.
    }
}

// ---------------------------------------------------------------------------
// walk_* free functions
// ---------------------------------------------------------------------------

fn walk_lines<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, lines: &'ast [Line]) {
    for line in lines {
        for stmt in line {
            v.visit_statement(stmt);
        }
    }
}

/// Walk all statements in all lines of a program. Call from [`Visit::visit_program`] overrides.
pub fn walk_program<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, program: &'ast Program) {
    walk_lines(v, &program.lines);
}

/// Visit the expression inside a statement. Call from [`Visit::visit_statement`] overrides.
pub fn walk_statement<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, stmt: &'ast Statement) {
    v.visit_expression(&stmt.expression);
}

/// Recurse into child expressions. Call from [`Visit::visit_expression`] overrides.
pub fn walk_expression<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, expr: &'ast Expression) {
    match expr {
        Expression::Command(cmd) => v.visit_command(cmd),
        Expression::Compound { body, redirects } => {
            v.visit_compound_command(body);
            for r in redirects {
                v.visit_redirect(r);
            }
        }
        Expression::FunctionDef(fndef) => v.visit_function_def(fndef),
        Expression::And { left, right } | Expression::Or { left, right } => {
            v.visit_expression(left);
            v.visit_expression(right);
        }
        Expression::Pipe { left, right, .. } => {
            v.visit_expression(left);
            v.visit_expression(right);
        }
        Expression::Not(inner) => {
            v.visit_expression(inner);
        }
    }
}

/// Visit assignments, arguments, and redirects. Call from [`Visit::visit_command`] overrides.
pub fn walk_command<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, cmd: &'ast Command) {
    for assignment in &cmd.assignments {
        v.visit_assignment(assignment);
    }
    for arg in &cmd.arguments {
        v.visit_argument(arg);
    }
    for redirect in &cmd.redirects {
        v.visit_redirect(redirect);
    }
}

/// Visit body and sub-structures of a compound command. Call from [`Visit::visit_compound_command`] overrides.
pub fn walk_compound_command<'ast, V: Visit<'ast> + ?Sized>(
    v: &mut V,
    compound: &'ast CompoundCommand,
) {
    match compound {
        CompoundCommand::BraceGroup { body, .. } | CompoundCommand::Subshell { body, .. } => {
            walk_lines(v, body);
        }
        CompoundCommand::ForClause { words, body, .. }
        | CompoundCommand::BashSelectClause { words, body, .. } => {
            if let Some(word_list) = words {
                for w in word_list {
                    v.visit_word(w);
                }
            }
            walk_lines(v, body);
        }
        CompoundCommand::CaseClause { word, arms, .. } => {
            v.visit_word(word);
            for arm in arms {
                v.visit_case_arm(arm);
            }
        }
        CompoundCommand::IfClause {
            condition,
            then_body,
            elifs,
            else_body,
            ..
        } => {
            walk_lines(v, condition);
            walk_lines(v, then_body);
            for elif in elifs {
                v.visit_elif_clause(elif);
            }
            if let Some(else_lines) = else_body {
                walk_lines(v, else_lines);
            }
        }
        CompoundCommand::WhileClause {
            condition, body, ..
        }
        | CompoundCommand::UntilClause {
            condition, body, ..
        } => {
            walk_lines(v, condition);
            walk_lines(v, body);
        }
        CompoundCommand::BashDoubleBracket { .. }
        | CompoundCommand::BashArithmeticCommand { .. } => {}
        CompoundCommand::BashCoproc { body, .. } => {
            v.visit_expression(body);
        }
        CompoundCommand::BashArithmeticFor { body, .. } => {
            walk_lines(v, body);
        }
    }
}

/// Visit body and redirects of a function definition. Call from [`Visit::visit_function_def`] overrides.
pub fn walk_function_def<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, fndef: &'ast FunctionDef) {
    v.visit_compound_command(&fndef.body);
    for r in &fndef.redirects {
        v.visit_redirect(r);
    }
}

/// Visit the target word inside a redirect. Call from [`Visit::visit_redirect`] overrides.
pub fn walk_redirect<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, redirect: &'ast Redirect) {
    match &redirect.kind {
        RedirectKind::Input(w)
        | RedirectKind::Output(w)
        | RedirectKind::Append(w)
        | RedirectKind::Clobber(w)
        | RedirectKind::ReadWrite(w)
        | RedirectKind::DupInput(w)
        | RedirectKind::DupOutput(w)
        | RedirectKind::BashHereString(w)
        | RedirectKind::BashOutputAll(w)
        | RedirectKind::BashAppendAll(w) => {
            v.visit_word(w);
        }
        RedirectKind::HereDoc { .. } => {}
    }
}

/// Visit the value word(s) in an assignment. Call from [`Visit::visit_assignment`] overrides.
pub fn walk_assignment<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, assignment: &'ast Assignment) {
    match &assignment.value {
        AssignmentValue::Scalar(w) => v.visit_word(w),
        AssignmentValue::BashArray(words) => {
            for w in words {
                v.visit_word(w);
            }
        }
    }
}

/// Visit patterns and body in a case arm. Call from [`Visit::visit_case_arm`] overrides.
pub fn walk_case_arm<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, arm: &'ast CaseArm) {
    for pattern in &arm.patterns {
        v.visit_word(pattern);
    }
    walk_lines(v, &arm.body);
}

/// Visit condition and body of an elif clause. Call from [`Visit::visit_elif_clause`] overrides.
pub fn walk_elif_clause<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, elif: &'ast ElifClause) {
    walk_lines(v, &elif.condition);
    walk_lines(v, &elif.body);
}

/// Visit the word inside an argument (atoms are not entered). Call from [`Visit::visit_argument`] overrides.
pub fn walk_argument<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, argument: &'ast Argument) {
    match argument {
        Argument::Word(w) => v.visit_word(w),
        Argument::Atom(_) => {
            // Process substitution body is not traversed by default.
        }
    }
}

#[cfg(test)]
#[path = "visit_tests.rs"]
mod tests;
