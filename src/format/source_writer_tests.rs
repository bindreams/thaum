use super::SourceWriter;

testutil::default_labels!(lex, parse);

fn fmt(source: &str) -> String {
    let program = crate::parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    SourceWriter::format_program(&program)
}

fn fmt_bash(source: &str) -> String {
    let program = crate::parse_with(source, crate::Dialect::Bash).unwrap_or_else(|e| panic!("parse failed: {e}"));
    SourceWriter::format_program(&program)
}

// Simple commands -----------------------------------------------------------------------------------------------------

#[testutil::test]
fn simple_command() {
    assert_eq!(fmt("echo hello world"), "echo hello world\n");
}

#[testutil::test]
fn assignment_only() {
    assert_eq!(fmt("x=42"), "x=42\n");
}

#[testutil::test]
fn assignment_and_command() {
    assert_eq!(fmt("FOO=bar echo hello"), "FOO=bar echo hello\n");
}

// Pipes, &&, || -------------------------------------------------------------------------------------------------------

#[testutil::test]
fn pipe() {
    assert_eq!(fmt("echo hi | grep h"), "echo hi | grep h\n");
}

#[testutil::test]
fn and_list() {
    assert_eq!(fmt("true && echo yes"), "true && echo yes\n");
}

#[testutil::test]
fn or_list() {
    assert_eq!(fmt("false || echo no"), "false || echo no\n");
}

#[testutil::test]
fn not_expression() {
    assert_eq!(fmt("! false"), "! false\n");
}

// Compound commands ---------------------------------------------------------------------------------------------------

#[testutil::test]
fn brace_group() {
    assert_eq!(fmt("{ echo hello; }"), "{\n    echo hello\n}\n");
}

#[testutil::test]
fn subshell() {
    assert_eq!(fmt("(echo hello)"), "(\n    echo hello\n)\n");
}

#[testutil::test]
fn if_then_fi() {
    assert_eq!(fmt("if true; then echo yes; fi"), "if true; then\n    echo yes\nfi\n");
}

#[testutil::test]
fn if_else() {
    assert_eq!(
        fmt("if true; then echo yes; else echo no; fi"),
        "if true; then\n    echo yes\nelse\n    echo no\nfi\n"
    );
}

#[testutil::test]
fn if_elif_else() {
    assert_eq!(
        fmt("if false; then echo a; elif true; then echo b; else echo c; fi"),
        "if false; then\n    echo a\nelif true; then\n    echo b\nelse\n    echo c\nfi\n"
    );
}

#[testutil::test]
fn while_loop() {
    assert_eq!(
        fmt("while true; do echo loop; done"),
        "while true; do\n    echo loop\ndone\n"
    );
}

#[testutil::test]
fn until_loop() {
    assert_eq!(
        fmt("until false; do echo loop; done"),
        "until false; do\n    echo loop\ndone\n"
    );
}

#[testutil::test]
fn for_loop() {
    assert_eq!(
        fmt("for x in a b c; do echo $x; done"),
        "for x in a b c; do\n    echo $x\ndone\n"
    );
}

#[testutil::test]
fn case_statement() {
    let input = "case $x in a) echo A;; b) echo B;; esac";
    let expected = "case $x in\n    a)\n        echo A\n    ;;\n    b)\n        echo B\n    ;;\nesac\n";
    assert_eq!(fmt(input), expected);
}

// Redirects -----------------------------------------------------------------------------------------------------------

#[testutil::test]
fn redirect_output() {
    assert_eq!(fmt("echo hi > file"), "echo hi > file\n");
}

#[testutil::test]
fn redirect_append() {
    assert_eq!(fmt("echo hi >> file"), "echo hi >> file\n");
}

#[testutil::test]
fn redirect_input() {
    assert_eq!(fmt("cat < file"), "cat < file\n");
}

// Words and expansions ------------------------------------------------------------------------------------------------

#[testutil::test]
fn parameter_simple() {
    assert_eq!(fmt("echo $x"), "echo $x\n");
}

#[testutil::test]
fn parameter_braced() {
    assert_eq!(fmt("echo ${x}"), "echo ${x}\n");
}

#[testutil::test]
fn parameter_default() {
    assert_eq!(fmt("echo ${x:-default}"), "echo ${x:-default}\n");
}

#[testutil::test]
fn parameter_length() {
    assert_eq!(fmt("echo ${#x}"), "echo ${#x}\n");
}

#[testutil::test]
fn command_substitution() {
    assert_eq!(fmt("echo $(ls)"), "echo $(ls)\n");
}

#[testutil::test]
fn arithmetic_expansion() {
    assert_eq!(fmt("echo $((1 + 2))"), "echo $((1 + 2))\n");
}

#[testutil::test]
fn single_quoted() {
    assert_eq!(fmt("echo 'hello world'"), "echo 'hello world'\n");
}

#[testutil::test]
fn double_quoted() {
    assert_eq!(fmt("echo \"hello $x\""), "echo \"hello $x\"\n");
}

// Bash features -------------------------------------------------------------------------------------------------------

#[testutil::test]
fn double_bracket() {
    assert_eq!(fmt_bash("[[ -f foo ]]"), "[[ -f foo ]]\n");
}

#[testutil::test]
fn arithmetic_command() {
    assert_eq!(fmt_bash("(( x + 1 ))"), "(( x + 1 ))\n");
}

#[testutil::test]
fn function_def() {
    assert_eq!(fmt_bash("f() { echo hello; }"), "f ()\n{\n    echo hello\n}\n");
}

// Full declare -f style output ----------------------------------------------------------------------------------------

#[testutil::test]
fn format_function_via_stored() {
    use crate::exec::environment::StoredFunction;
    let program = crate::parse_with("f() { echo hello; }", crate::Dialect::Bash).unwrap();
    // Extract the function def from the parsed program
    let stmt = &program.lines[0][0];
    let fndef = match &stmt.expression {
        crate::ast::Expression::FunctionDef(f) => f,
        _ => panic!("expected function def"),
    };
    let stored = StoredFunction::from(fndef);
    let result = SourceWriter::format_function("f", &stored);
    assert_eq!(result, "f ()\n{\n    echo hello\n}\n");
}
