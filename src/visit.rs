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
    fn visit_program(&mut self, program: &'ast Program) {
        walk_program(self, program);
    }

    fn visit_statement(&mut self, statement: &'ast Statement) {
        walk_statement(self, statement);
    }

    fn visit_expression(&mut self, expression: &'ast Expression) {
        walk_expression(self, expression);
    }

    fn visit_command(&mut self, command: &'ast Command) {
        walk_command(self, command);
    }

    fn visit_compound_command(&mut self, compound: &'ast CompoundCommand) {
        walk_compound_command(self, compound);
    }

    fn visit_function_def(&mut self, function_def: &'ast FunctionDef) {
        walk_function_def(self, function_def);
    }

    fn visit_redirect(&mut self, redirect: &'ast Redirect) {
        walk_redirect(self, redirect);
    }

    fn visit_assignment(&mut self, assignment: &'ast Assignment) {
        walk_assignment(self, assignment);
    }

    fn visit_case_arm(&mut self, arm: &'ast CaseArm) {
        walk_case_arm(self, arm);
    }

    fn visit_elif_clause(&mut self, elif: &'ast ElifClause) {
        walk_elif_clause(self, elif);
    }

    fn visit_argument(&mut self, argument: &'ast Argument) {
        walk_argument(self, argument);
    }

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

pub fn walk_program<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, program: &'ast Program) {
    walk_lines(v, &program.lines);
}

pub fn walk_statement<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, stmt: &'ast Statement) {
    v.visit_expression(&stmt.expression);
}

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

pub fn walk_function_def<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, fndef: &'ast FunctionDef) {
    v.visit_compound_command(&fndef.body);
    for r in &fndef.redirects {
        v.visit_redirect(r);
    }
}

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

pub fn walk_case_arm<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, arm: &'ast CaseArm) {
    for pattern in &arm.patterns {
        v.visit_word(pattern);
    }
    walk_lines(v, &arm.body);
}

pub fn walk_elif_clause<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, elif: &'ast ElifClause) {
    walk_lines(v, &elif.condition);
    walk_lines(v, &elif.body);
}

pub fn walk_argument<'ast, V: Visit<'ast> + ?Sized>(v: &mut V, argument: &'ast Argument) {
    match argument {
        Argument::Word(w) => v.visit_word(w),
        Argument::Atom(_) => {
            // Process substitution body is not traversed by default.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::Dialect;

    fn parse_bash(input: &str) -> Program {
        crate::parse_with(input, Dialect::Bash).unwrap()
    }

    // -- Statement counter --------------------------------------------------

    struct StmtCounter(usize);

    impl<'ast> Visit<'ast> for StmtCounter {
        fn visit_statement(&mut self, stmt: &'ast Statement) {
            self.0 += 1;
            walk_statement(self, stmt);
        }
    }

    #[test]
    fn count_statements() {
        let prog = parse_bash("echo a; echo b; echo c");
        let mut c = StmtCounter(0);
        c.visit_program(&prog);
        assert_eq!(c.0, 3);
    }

    #[test]
    fn count_statements_nested() {
        let prog = parse_bash("if true; then echo a; echo b; fi");
        let mut c = StmtCounter(0);
        c.visit_program(&prog);
        // 1 (top-level if) + 1 (condition: true) + 2 (then body: echo a, echo b)
        assert_eq!(c.0, 4);
    }

    // -- Command name collector ---------------------------------------------

    struct CmdNames(Vec<String>);

    impl<'ast> Visit<'ast> for CmdNames {
        fn visit_command(&mut self, cmd: &'ast Command) {
            if let Some(name) = cmd.arguments.first().and_then(|a| a.try_to_static_string()) {
                self.0.push(name);
            }
            walk_command(self, cmd);
        }
    }

    #[test]
    fn collect_pipeline_commands() {
        let prog = parse_bash("ls -la | grep foo | wc -l");
        let mut v = CmdNames(vec![]);
        v.visit_program(&prog);
        assert_eq!(v.0, vec!["ls", "grep", "wc"]);
    }

    #[test]
    fn collect_if_body_commands() {
        let prog = parse_bash("if true; then echo hello; elif false; then echo bye; fi");
        let mut v = CmdNames(vec![]);
        v.visit_program(&prog);
        assert_eq!(v.0, vec!["true", "echo", "false", "echo"]);
    }

    #[test]
    fn collect_for_body_commands() {
        let prog = parse_bash("for x in a b c; do echo $x; done");
        let mut v = CmdNames(vec![]);
        v.visit_program(&prog);
        assert_eq!(v.0, vec!["echo"]);
    }

    #[test]
    fn collect_while_commands() {
        let prog = parse_bash("while read line; do echo $line; done");
        let mut v = CmdNames(vec![]);
        v.visit_program(&prog);
        assert_eq!(v.0, vec!["read", "echo"]);
    }

    #[test]
    fn collect_function_body_commands() {
        let prog = parse_bash("greet() { echo hello; echo world; }");
        let mut v = CmdNames(vec![]);
        v.visit_program(&prog);
        assert_eq!(v.0, vec!["echo", "echo"]);
    }

    #[test]
    fn collect_case_body_commands() {
        let prog = parse_bash("case $x in a) echo a;; b) echo b;; esac");
        let mut v = CmdNames(vec![]);
        v.visit_program(&prog);
        assert_eq!(v.0, vec!["echo", "echo"]);
    }

    #[test]
    fn collect_and_or_commands() {
        let prog = parse_bash("true && echo ok || echo fail");
        let mut v = CmdNames(vec![]);
        v.visit_program(&prog);
        assert_eq!(v.0, vec!["true", "echo", "echo"]);
    }

    #[test]
    fn collect_negated_command() {
        let prog = parse_bash("! false");
        let mut v = CmdNames(vec![]);
        v.visit_program(&prog);
        assert_eq!(v.0, vec!["false"]);
    }

    // -- Override without walk stops descent ---------------------------------

    struct TopLevelOnly(Vec<String>);

    impl<'ast> Visit<'ast> for TopLevelOnly {
        fn visit_command(&mut self, cmd: &'ast Command) {
            if let Some(name) = cmd.arguments.first().and_then(|a| a.try_to_static_string()) {
                self.0.push(name);
            }
            // Deliberately do NOT call walk_command — stops descent into args/redirects.
        }

        fn visit_compound_command(&mut self, _compound: &'ast CompoundCommand) {
            // Deliberately do NOT call walk_compound_command — stops descent into body.
        }
    }

    #[test]
    fn override_stops_descent() {
        let prog = parse_bash("if true; then echo inner; fi");
        let mut v = TopLevelOnly(vec![]);
        v.visit_program(&prog);
        // Only the top-level compound is visited, but we don't descend into it,
        // so "true" and "echo" from inside the if are not collected.
        assert!(v.0.is_empty());
    }
}
