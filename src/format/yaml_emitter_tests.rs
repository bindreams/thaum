use super::*;
use crate::format::yaml_value::{MappingBuilder, YamlValue};

#[test]
fn emit_simple_mapping() {
    let value = YamlValue::mapping()
        .raw("type", "Command")
        .raw("name", "echo")
        .build();
    assert_eq!(emit(&value), "type: Command\nname: echo\n");
}

#[test]
fn emit_nested_mapping() {
    let inner = YamlValue::mapping().raw("type", "Literal").build();
    let value = YamlValue::mapping().value("body", inner).build();
    assert_eq!(emit(&value), "body:\n  type: Literal\n");
}

#[test]
fn emit_scalar_sequence() {
    let seq = YamlValue::Sequence(vec![
        YamlValue::scalar("echo"),
        YamlValue::scalar("hello"),
    ]);
    let value = YamlValue::mapping().value("arguments", seq).build();
    assert_eq!(
        emit(&value),
        "arguments:\n  - echo\n  - hello\n"
    );
}

#[test]
fn emit_sequence_of_mappings() {
    let items = vec![
        YamlValue::mapping()
            .raw("source", "<stdin>:1:1")
            .raw("type", "Command")
            .build(),
        YamlValue::mapping()
            .raw("source", "<stdin>:1:10")
            .raw("type", "Command")
            .build(),
    ];
    let value = YamlValue::mapping()
        .value("statements", YamlValue::Sequence(items))
        .build();
    assert_eq!(
        emit(&value),
        "\
statements:
  - source: <stdin>:1:1
    type: Command
  - source: <stdin>:1:10
    type: Command
"
    );
}

#[test]
fn emit_empty_sequence() {
    let value = YamlValue::mapping()
        .value("arguments", YamlValue::Sequence(vec![]))
        .build();
    assert_eq!(emit(&value), "arguments: []\n");
}

#[test]
fn emit_block_scalar() {
    let value = YamlValue::mapping()
        .value("body", YamlValue::block_scalar("hello world"))
        .build();
    assert_eq!(emit(&value), "body:\n  |\n  hello world\n");
}

#[test]
fn emit_block_scalar_multiline() {
    let value = YamlValue::mapping()
        .value("body", YamlValue::block_scalar("line1\nline2"))
        .build();
    assert_eq!(emit(&value), "body:\n  |\n  line1\n  line2\n");
}

#[test]
fn emit_yaml_escape_empty() {
    assert_eq!(yaml_escape(""), "\"\"");
}

#[test]
fn emit_yaml_escape_plain() {
    assert_eq!(yaml_escape("echo"), "echo");
}

#[test]
fn emit_yaml_escape_colon() {
    assert_eq!(yaml_escape("key:value"), "\"key:value\"");
}

#[test]
fn emit_yaml_escape_true() {
    assert_eq!(yaml_escape("true"), "\"true\"");
}

#[test]
fn emit_yaml_escape_number() {
    assert_eq!(yaml_escape("42"), "\"42\"");
}

#[test]
fn emit_yaml_escape_backslash() {
    assert_eq!(yaml_escape("a\\b"), "\"a\\\\b\"");
}

#[test]
fn emit_nested_indentation() {
    let inner_seq = YamlValue::Sequence(vec![
        YamlValue::scalar("echo"),
        YamlValue::scalar("hello"),
    ]);
    let stmt = YamlValue::mapping()
        .raw("source", "<stdin>:1:1")
        .raw("type", "Command")
        .value("arguments", inner_seq)
        .build();
    let value = YamlValue::mapping()
        .value("statements", YamlValue::Sequence(vec![stmt]))
        .build();
    assert_eq!(
        emit(&value),
        "\
statements:
  - source: <stdin>:1:1
    type: Command
    arguments:
      - echo
      - hello
"
    );
}

#[test]
fn emit_escaped_scalar_in_sequence() {
    let seq = YamlValue::Sequence(vec![
        YamlValue::scalar("true"),
        YamlValue::scalar("hello:world"),
    ]);
    let value = YamlValue::mapping().value("items", seq).build();
    assert_eq!(
        emit(&value),
        "items:\n  - \"true\"\n  - \"hello:world\"\n"
    );
}

#[test]
fn emit_raw_vs_escaped() {
    let value = YamlValue::mapping()
        .raw("type", "Command")
        .scalar("value", "true")
        .build();
    assert_eq!(emit(&value), "type: Command\nvalue: \"true\"\n");
}
