//! Immutable AST visitor.
//!
//! Each `visit_*` trait method has a default that delegates to the
//! corresponding free `walk_*` function, which visits child nodes.
//! Override a method to intercept a node; call the `walk_*` function
//! inside your override to continue descending.
//!
//! The visitor covers the full AST: statements, expressions, compound
//! commands, words, fragments, parameter expansions, arithmetic
//! expressions, and test expressions.
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

    /// Visit a word. Default: visits each fragment.
    fn visit_word(&mut self, word: &'ast Word) {
        walk_word(self, word);
    }

    /// Visit a standalone atom (e.g., process substitution). Default: visits body statements.
    fn visit_atom(&mut self, atom: &'ast Atom) {
        walk_atom(self, atom);
    }

    /// Visit a word fragment. Default: descends into nested structures.
    fn visit_fragment(&mut self, fragment: &'ast Fragment) {
        walk_fragment(self, fragment);
    }

    /// Visit a parameter expansion. Default: visits the argument word if present.
    fn visit_parameter_expansion(&mut self, expansion: &'ast ParameterExpansion) {
        walk_parameter_expansion(self, expansion);
    }

    /// Visit an arithmetic expression. Default: recurses into sub-expressions.
    fn visit_arith_expr(&mut self, expr: &'ast ArithExpr) {
        walk_arith_expr(self, expr);
    }

    /// Visit a `[[ ]]` test expression. Default: recurses into sub-expressions.
    fn visit_bash_test_expr(&mut self, expr: &'ast BashTestExpr) {
        walk_bash_test_expr(self, expr);
    }
}

// walk_* free functions ===============================================================================================

/// Walk all statements in all lines. Call from overrides that need to visit line lists.
pub fn walk_lines<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, lines: &'ast [Line]) {
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
pub fn walk_compound_command<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, compound: &'ast CompoundCommand) {
    match compound {
        CompoundCommand::BraceGroup { body, .. } | CompoundCommand::Subshell { body, .. } => {
            walk_lines(v, body);
        }
        CompoundCommand::ForClause { words, body, .. } | CompoundCommand::BashSelectClause { words, body, .. } => {
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
        CompoundCommand::WhileClause { condition, body, .. } | CompoundCommand::UntilClause { condition, body, .. } => {
            walk_lines(v, condition);
            walk_lines(v, body);
        }
        CompoundCommand::BashDoubleBracket { expression, .. } => {
            v.visit_bash_test_expr(expression);
        }
        CompoundCommand::BashArithmeticCommand { expression, .. } => {
            v.visit_arith_expr(expression);
        }
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
        AssignmentValue::BashArray(elems) => {
            for elem in elems {
                match elem {
                    ArrayElement::Plain(w) => v.visit_word(w),
                    ArrayElement::Subscripted { value, .. } => v.visit_word(value),
                }
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

/// Visit the inner word or atom. Call from [`Visit::visit_argument`] overrides.
pub fn walk_argument<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, argument: &'ast Argument) {
    match argument {
        Argument::Word(w) => v.visit_word(w),
        Argument::Atom(a) => v.visit_atom(a),
    }
}

/// Visit the body statements inside a process substitution atom.
pub fn walk_atom<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, atom: &'ast Atom) {
    match atom {
        Atom::BashProcessSubstitution { body, .. } => {
            for stmt in body {
                v.visit_statement(stmt);
            }
        }
    }
}

/// Visit each fragment in a word.
pub fn walk_word<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, word: &'ast Word) {
    for fragment in &word.parts {
        v.visit_fragment(fragment);
    }
}

/// Descend into nested structures within a fragment. Call from [`Visit::visit_fragment`] overrides.
pub fn walk_fragment<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, fragment: &'ast Fragment) {
    match fragment {
        Fragment::Literal(_) | Fragment::SingleQuoted(_) | Fragment::BashAnsiCQuoted(_) => {}
        Fragment::DoubleQuoted(parts) => {
            for part in parts {
                v.visit_fragment(part);
            }
        }
        Fragment::Parameter(expansion) => v.visit_parameter_expansion(expansion),
        Fragment::CommandSubstitution(stmts) => {
            for stmt in stmts {
                v.visit_statement(stmt);
            }
        }
        Fragment::ArithmeticExpansion(expr) => v.visit_arith_expr(expr),
        Fragment::Glob(_) | Fragment::TildePrefix(_) => {}
        Fragment::BashLocaleQuoted { parts, .. } => {
            for part in parts {
                v.visit_fragment(part);
            }
        }
        Fragment::BashExtGlob { .. } | Fragment::BashBraceExpansion(_) => {}
    }
}

/// Visit the argument word inside a parameter expansion if present.
pub fn walk_parameter_expansion<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, expansion: &'ast ParameterExpansion) {
    match expansion {
        ParameterExpansion::Simple(_) => {}
        ParameterExpansion::Complex { argument, .. } => {
            if let Some(arg) = argument {
                v.visit_word(arg);
            }
        }
    }
}

/// Recurse into sub-expressions of an arithmetic expression.
pub fn walk_arith_expr<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, expr: &'ast ArithExpr) {
    match expr {
        ArithExpr::Number(_) | ArithExpr::Variable(_) => {}
        ArithExpr::Binary { left, right, .. } | ArithExpr::Comma { left, right } => {
            v.visit_arith_expr(left);
            v.visit_arith_expr(right);
        }
        ArithExpr::UnaryPrefix { operand, .. } | ArithExpr::UnaryPostfix { operand, .. } => {
            v.visit_arith_expr(operand);
        }
        ArithExpr::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            v.visit_arith_expr(condition);
            v.visit_arith_expr(then_expr);
            v.visit_arith_expr(else_expr);
        }
        ArithExpr::Assignment { value, .. } => v.visit_arith_expr(value),
        ArithExpr::Group(inner) => v.visit_arith_expr(inner),
    }
}

/// Recurse into sub-expressions of a `[[ ]]` test expression.
pub fn walk_bash_test_expr<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, expr: &'ast BashTestExpr) {
    match expr {
        BashTestExpr::Unary { arg, .. } => v.visit_word(arg),
        BashTestExpr::Binary { left, right, .. } => {
            v.visit_word(left);
            v.visit_word(right);
        }
        BashTestExpr::And { left, right } | BashTestExpr::Or { left, right } => {
            v.visit_bash_test_expr(left);
            v.visit_bash_test_expr(right);
        }
        BashTestExpr::Not(inner) | BashTestExpr::Group(inner) => v.visit_bash_test_expr(inner),
        BashTestExpr::Word(w) => v.visit_word(w),
    }
}

#[cfg(test)]
#[path = "visit_tests.rs"]
mod tests;
