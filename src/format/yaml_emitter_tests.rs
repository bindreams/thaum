use super::*;
use crate::format::yaml_value::YamlValue;

// Round-trip safety ---------------------------------------------------------------------------------------------------
//
// Emitted YAML must parse back as strings (not numbers, booleans, or null)
// when string values are used.

/// Parse emitted YAML and extract a scalar value by key, verifying it's a
/// String in the yaml_rust2 data model.
fn roundtrip_string(key: &str, value: &str) {
    let yaml_value = YamlValue::mapping().scalar(key, value).build();
    let emitted = emit(&yaml_value);
    let docs = yaml_rust2::YamlLoader::load_from_str(&emitted)
        .unwrap_or_else(|e| panic!("emitted YAML is invalid: {e}\n---\n{emitted}"));
    let doc = &docs[0];
    let actual = &doc[key];
    assert!(
        matches!(actual, yaml_rust2::Yaml::String(_)),
        "expected String for key {key:?} with value {value:?}, got {actual:?}\nemitted: {emitted}"
    );
    assert_eq!(
        actual.as_str().unwrap(),
        value,
        "round-trip value mismatch for key {key:?}"
    );
}

#[testutil::test]
fn roundtrip_number_stays_string() {
    roundtrip_string("value", "9");
    roundtrip_string("value", "0");
    roundtrip_string("value", "42");
    roundtrip_string("value", "3.14");
    roundtrip_string("value", "0x1F");
    roundtrip_string("value", "-1");
}

#[testutil::test]
fn roundtrip_bool_stays_string() {
    roundtrip_string("strip_tabs", "true");
    roundtrip_string("strip_tabs", "false");
}

#[testutil::test]
fn roundtrip_null_stays_string() {
    roundtrip_string("value", "~");
    roundtrip_string("value", "null");
}

#[testutil::test]
fn roundtrip_yaml_special_chars_stay_string() {
    roundtrip_string("op", "*");
    roundtrip_string("op", ">");
    roundtrip_string("op", "<");
    roundtrip_string("op", "|");
    roundtrip_string("op", "!");
    roundtrip_string("op", "&");
    roundtrip_string("op", "%");
    roundtrip_string("direction", "<");
    roundtrip_string("direction", ">");
    roundtrip_string("op", "-");
    roundtrip_string("op", "+");
    roundtrip_string("op", "++");
    roundtrip_string("op", "--");
    roundtrip_string("op", "==");
    roundtrip_string("op", "!=");
    roundtrip_string("op", "=~");
    roundtrip_string("op", "&&");
    roundtrip_string("op", "||");
}

#[testutil::test]
fn roundtrip_param_operators_stay_string() {
    roundtrip_string("operator", ":-");
    roundtrip_string("operator", ":=");
    roundtrip_string("operator", ":?");
    roundtrip_string("operator", ":+");
    roundtrip_string("operator", "#");
    roundtrip_string("operator", "%");
    roundtrip_string("operator", "%%");
    roundtrip_string("operator", "##");
}

/// Verify that yaml_rust2 (our reader) implements YAML 1.2, where bare
/// `no`, `yes`, `on`, `off` are strings — not booleans as in YAML 1.1.
#[testutil::test]
fn yaml_reader_is_1_2() {
    let docs = yaml_rust2::YamlLoader::load_from_str("value: no").unwrap();
    let val = &docs[0]["value"];
    assert!(
        matches!(val, yaml_rust2::Yaml::String(_)),
        "expected String(\"no\") under YAML 1.2, got {val:?} — reader may be YAML 1.1"
    );
    assert_eq!(val.as_str().unwrap(), "no");
}

#[testutil::test]
fn emit_simple_mapping() {
    let value = YamlValue::mapping().raw("type", "Command").raw("name", "echo").build();
    assert_eq!(emit(&value), "type: Command\nname: echo\n");
}

#[testutil::test]
fn emit_nested_mapping() {
    let inner = YamlValue::mapping().raw("type", "Literal").build();
    let value = YamlValue::mapping().value("body", inner).build();
    assert_eq!(emit(&value), "body:\n  type: Literal\n");
}

#[testutil::test]
fn emit_scalar_sequence() {
    let seq = YamlValue::Sequence(vec![YamlValue::scalar("echo"), YamlValue::scalar("hello")]);
    let value = YamlValue::mapping().value("arguments", seq).build();
    assert_eq!(emit(&value), "arguments:\n  - echo\n  - hello\n");
}

#[testutil::test]
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

#[testutil::test]
fn emit_empty_sequence() {
    let value = YamlValue::mapping()
        .value("arguments", YamlValue::Sequence(vec![]))
        .build();
    assert_eq!(emit(&value), "arguments: []\n");
}

#[testutil::test]
fn emit_block_scalar() {
    let value = YamlValue::mapping()
        .value("body", YamlValue::block_scalar("hello world"))
        .build();
    assert_eq!(emit(&value), "body:\n  |\n  hello world\n");
}

#[testutil::test]
fn emit_block_scalar_multiline() {
    let value = YamlValue::mapping()
        .value("body", YamlValue::block_scalar("line1\nline2"))
        .build();
    assert_eq!(emit(&value), "body:\n  |\n  line1\n  line2\n");
}

#[testutil::test]
fn emit_yaml_escape_empty() {
    assert_eq!(yaml_escape(""), "\"\"");
}

#[testutil::test]
fn emit_yaml_escape_plain() {
    assert_eq!(yaml_escape("echo"), "echo");
}

#[testutil::test]
fn emit_yaml_escape_colon() {
    assert_eq!(yaml_escape("key:value"), "\"key:value\"");
}

#[testutil::test]
fn emit_yaml_escape_true() {
    assert_eq!(yaml_escape("true"), "\"true\"");
}

#[testutil::test]
fn emit_yaml_escape_number() {
    assert_eq!(yaml_escape("42"), "\"42\"");
}

#[testutil::test]
fn emit_yaml_escape_backslash() {
    assert_eq!(yaml_escape("a\\b"), "\"a\\\\b\"");
}

#[testutil::test]
fn emit_nested_indentation() {
    let inner_seq = YamlValue::Sequence(vec![YamlValue::scalar("echo"), YamlValue::scalar("hello")]);
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

#[testutil::test]
fn emit_escaped_scalar_in_sequence() {
    let seq = YamlValue::Sequence(vec![YamlValue::scalar("true"), YamlValue::scalar("hello:world")]);
    let value = YamlValue::mapping().value("items", seq).build();
    assert_eq!(emit(&value), "items:\n  - \"true\"\n  - \"hello:world\"\n");
}

#[testutil::test]
fn emit_raw_vs_escaped() {
    let value = YamlValue::mapping()
        .raw("type", "Command")
        .scalar("value", "true")
        .build();
    assert_eq!(emit(&value), "type: Command\nvalue: \"true\"\n");
}
