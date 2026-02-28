use super::*;
use crate::dialect::Dialect;

fn parse_bash(input: &str) -> Program {
    crate::parse_with(input, Dialect::Bash).unwrap()
}

// Statement counter ---------------------------------------------------------------------------------------------------

struct StmtCounter(usize);

impl<'ast> Visit<'ast> for StmtCounter {
    fn visit_statement(&mut self, stmt: &'ast Statement) {
        self.0 += 1;
        walk_statement(self, stmt);
    }
}

#[testutil::test]
fn count_statements() {
    let prog = parse_bash("echo a; echo b; echo c");
    let mut c = StmtCounter(0);
    c.visit_program(&prog);
    assert_eq!(c.0, 3);
}

#[testutil::test]
fn count_statements_nested() {
    let prog = parse_bash("if true; then echo a; echo b; fi");
    let mut c = StmtCounter(0);
    c.visit_program(&prog);
    // 1 (top-level if) + 1 (condition: true) + 2 (then body: echo a, echo b)
    assert_eq!(c.0, 4);
}

// Command name collector ----------------------------------------------------------------------------------------------

struct CmdNames(Vec<String>);

impl<'ast> Visit<'ast> for CmdNames {
    fn visit_command(&mut self, cmd: &'ast Command) {
        if let Some(name) = cmd.arguments.first().and_then(|a| a.try_to_static_string()) {
            self.0.push(name);
        }
        walk_command(self, cmd);
    }
}

#[testutil::test]
fn collect_pipeline_commands() {
    let prog = parse_bash("ls -la | grep foo | wc -l");
    let mut v = CmdNames(vec![]);
    v.visit_program(&prog);
    assert_eq!(v.0, vec!["ls", "grep", "wc"]);
}

#[testutil::test]
fn collect_if_body_commands() {
    let prog = parse_bash("if true; then echo hello; elif false; then echo bye; fi");
    let mut v = CmdNames(vec![]);
    v.visit_program(&prog);
    assert_eq!(v.0, vec!["true", "echo", "false", "echo"]);
}

#[testutil::test]
fn collect_for_body_commands() {
    let prog = parse_bash("for x in a b c; do echo $x; done");
    let mut v = CmdNames(vec![]);
    v.visit_program(&prog);
    assert_eq!(v.0, vec!["echo"]);
}

#[testutil::test]
fn collect_while_commands() {
    let prog = parse_bash("while read line; do echo $line; done");
    let mut v = CmdNames(vec![]);
    v.visit_program(&prog);
    assert_eq!(v.0, vec!["read", "echo"]);
}

#[testutil::test]
fn collect_function_body_commands() {
    let prog = parse_bash("greet() { echo hello; echo world; }");
    let mut v = CmdNames(vec![]);
    v.visit_program(&prog);
    assert_eq!(v.0, vec!["echo", "echo"]);
}

#[testutil::test]
fn collect_case_body_commands() {
    let prog = parse_bash("case $x in a) echo a;; b) echo b;; esac");
    let mut v = CmdNames(vec![]);
    v.visit_program(&prog);
    assert_eq!(v.0, vec!["echo", "echo"]);
}

#[testutil::test]
fn collect_and_or_commands() {
    let prog = parse_bash("true && echo ok || echo fail");
    let mut v = CmdNames(vec![]);
    v.visit_program(&prog);
    assert_eq!(v.0, vec!["true", "echo", "echo"]);
}

#[testutil::test]
fn collect_negated_command() {
    let prog = parse_bash("! false");
    let mut v = CmdNames(vec![]);
    v.visit_program(&prog);
    assert_eq!(v.0, vec!["false"]);
}

// Override without walk stops descent ---------------------------------------------------------------------------------

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

#[testutil::test]
fn override_stops_descent() {
    let prog = parse_bash("if true; then echo inner; fi");
    let mut v = TopLevelOnly(vec![]);
    v.visit_program(&prog);
    // Only the top-level compound is visited, but we don't descend into it,
    // so "true" and "echo" from inside the if are not collected.
    assert!(v.0.is_empty());
}
