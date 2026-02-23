use super::*;
use crate::dialect::Dialect;

fn parse_bash(input: &str) -> Program {
    crate::parse_with(input, Dialect::Bash).unwrap()
}

// Identity fold -------------------------------------------------------------------------------------------------------

struct Identity;
impl Fold for Identity {}

#[test]
fn fold_identity_preserves_ast() {
    let prog = parse_bash("echo hello; if true; then echo yes; fi");
    let original = prog.clone();
    let folded = Identity.fold_program(prog);
    assert_eq!(folded, original);
}

// Uppercaser ----------------------------------------------------------------------------------------------------------

struct Uppercaser;
impl Fold for Uppercaser {
    fn fold_word(&mut self, mut word: Word) -> Word {
        for part in &mut word.parts {
            if let Fragment::Literal(s) = part {
                *s = s.to_uppercase();
            }
        }
        word
    }
}

#[test]
fn fold_uppercases_literals() {
    let prog = parse_bash("echo hello world");
    let prog = Uppercaser.fold_program(prog);
    let cmd = match &prog.lines[0][0].expression {
        Expression::Command(c) => c,
        _ => panic!("expected command"),
    };
    assert_eq!(cmd.arguments[0].try_to_static_string(), Some("ECHO".into()));
    assert_eq!(cmd.arguments[1].try_to_static_string(), Some("HELLO".into()));
    assert_eq!(cmd.arguments[2].try_to_static_string(), Some("WORLD".into()));
}

#[test]
fn fold_descends_into_compound() {
    let prog = parse_bash("if true; then echo inner; fi");
    let prog = Uppercaser.fold_program(prog);
    // The fold should reach "inner" inside the if body
    let if_clause = match &prog.lines[0][0].expression {
        Expression::Compound { body, .. } => body,
        _ => panic!("expected compound"),
    };
    if let CompoundCommand::IfClause { then_body, .. } = if_clause {
        let cmd = match &then_body[0][0].expression {
            Expression::Command(c) => c,
            _ => panic!("expected command"),
        };
        assert_eq!(cmd.arguments[1].try_to_static_string(), Some("INNER".into()));
    } else {
        panic!("expected if clause");
    }
}

#[test]
fn fold_descends_into_pipeline() {
    let prog = parse_bash("echo a | grep b");
    let prog = Uppercaser.fold_program(prog);
    // Verify both sides of the pipe are folded
    fn first_arg(expr: &Expression) -> String {
        match expr {
            Expression::Command(c) => c.arguments[1].try_to_static_string().unwrap(),
            _ => panic!("expected command"),
        }
    }
    if let Expression::Pipe { left, right, .. } = &prog.lines[0][0].expression {
        assert_eq!(first_arg(left), "A");
        assert_eq!(first_arg(right), "B");
    } else {
        panic!("expected pipe");
    }
}
