//! Converts `YamlValue` trees into indented YAML text.

use std::fmt::Write;

use super::yaml_value::YamlValue;

/// Emit a `YamlValue` as a YAML string with indented block sequences.
///
/// The output uses 2-space indentation with sequence items indented
/// relative to their parent mapping key:
///
/// ```yaml
/// key:
///   - value1
///   - value2
/// ```
pub fn emit(value: &YamlValue) -> String {
    let mut buf = String::new();
    emit_mapping_entries(&mut buf, value, 0);
    buf
}

/// Emit the entries of a top-level mapping (no braces, no indent for the
/// root level). If the value is not a Mapping, emit it as a plain value.
fn emit_mapping_entries(buf: &mut String, value: &YamlValue, indent: usize) {
    match value {
        YamlValue::Mapping(entries) => {
            for (key, val) in entries {
                emit_kv(buf, key, val, indent);
            }
        }
        _ => emit_value(buf, value, indent),
    }
}

/// Emit a key-value pair at the given indent level.
fn emit_kv(buf: &mut String, key: &str, value: &YamlValue, indent: usize) {
    write_indent(buf, indent);
    emit_kv_after_prefix(buf, key, value, indent);
}

/// Emit a key-value pair assuming the cursor is already positioned
/// (after indent or after `- `). `indent` is the effective indent level
/// for child content.
fn emit_kv_after_prefix(buf: &mut String, key: &str, value: &YamlValue, indent: usize) {
    match value {
        YamlValue::Null => {
            let _ = writeln!(buf, "{key}: null");
        }
        YamlValue::Scalar(s) => {
            let _ = writeln!(buf, "{}: {}", key, yaml_escape(s));
        }
        YamlValue::RawScalar(s) => {
            let _ = writeln!(buf, "{key}: {s}");
        }
        YamlValue::BlockScalar(s) => {
            let _ = writeln!(buf, "{key}:");
            write_indent(buf, indent + 2);
            buf.push_str("|\n");
            for line in s.lines() {
                write_indent(buf, indent + 2);
                let _ = writeln!(buf, "{line}");
            }
        }
        YamlValue::Sequence(items) if items.is_empty() => {
            let _ = writeln!(buf, "{key}: []");
        }
        YamlValue::Sequence(items) => {
            let _ = writeln!(buf, "{key}:");
            for item in items {
                emit_seq_item(buf, item, indent + 2);
            }
        }
        YamlValue::Mapping(entries) => {
            let _ = writeln!(buf, "{key}:");
            for (k, v) in entries {
                emit_kv(buf, k, v, indent + 2);
            }
        }
    }
}

/// Emit a sequence item with `- ` prefix.
///
/// When the item is a Mapping, the first key-value pair is placed on the
/// same line as `- ` (e.g., `- source: <stdin>:1:1`). Subsequent entries
/// are indented to align with the first key.
fn emit_seq_item(buf: &mut String, item: &YamlValue, indent: usize) {
    match item {
        YamlValue::Mapping(entries) if !entries.is_empty() => {
            // First entry on same line as `- `
            write_indent(buf, indent);
            buf.push_str("- ");
            let (key, value) = &entries[0];
            emit_kv_after_prefix(buf, key, value, indent + 2);
            // Remaining entries at indent + 2
            for (key, value) in &entries[1..] {
                emit_kv(buf, key, value, indent + 2);
            }
        }
        YamlValue::Scalar(s) => {
            write_indent(buf, indent);
            let _ = writeln!(buf, "- {}", yaml_escape(s));
        }
        YamlValue::RawScalar(s) => {
            write_indent(buf, indent);
            let _ = writeln!(buf, "- {s}");
        }
        YamlValue::Null => {
            write_indent(buf, indent);
            let _ = writeln!(buf, "- null");
        }
        _ => {
            // Empty mapping, sequence of sequences, or block scalar in a list
            write_indent(buf, indent);
            buf.push_str("- ");
            emit_value(buf, item, indent + 2);
        }
    }
}

/// Emit a value without a key prefix. Used for non-mapping, non-kv contexts.
fn emit_value(buf: &mut String, value: &YamlValue, indent: usize) {
    match value {
        YamlValue::Null => {
            let _ = writeln!(buf, "null");
        }
        YamlValue::Scalar(s) => {
            let _ = writeln!(buf, "{}", yaml_escape(s));
        }
        YamlValue::RawScalar(s) => {
            let _ = writeln!(buf, "{s}");
        }
        YamlValue::BlockScalar(s) => {
            buf.push_str("|\n");
            for line in s.lines() {
                write_indent(buf, indent);
                let _ = writeln!(buf, "{line}");
            }
        }
        YamlValue::Sequence(items) => {
            for item in items {
                emit_seq_item(buf, item, indent);
            }
        }
        YamlValue::Mapping(entries) => {
            for (key, value) in entries {
                emit_kv(buf, key, value, indent);
            }
        }
    }
}

fn write_indent(buf: &mut String, level: usize) {
    for _ in 0..level {
        buf.push(' ');
    }
}

/// Check if a string looks like a YAML 1.2 integer that `parse::<f64>` might miss.
/// YAML 1.2 recognizes `0x1F` (hex), `0o17` (octal), `0b1010` (binary).
fn looks_like_yaml_int(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        return !oct.is_empty() && oct.chars().all(|c| matches!(c, '0'..='7'));
    }
    if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        return !bin.is_empty() && bin.chars().all(|c| matches!(c, '0' | '1'));
    }
    false
}

/// Escape a string for YAML output. Quotes it if it contains special chars.
fn yaml_escape(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quoting = s.contains(':')
        || s.contains('#')
        || s.contains('\'')
        || s.contains('"')
        || s.contains('\n')
        || s.contains('\\')
        || s.contains('[')
        || s.contains(']')
        || s.contains('{')
        || s.contains('}')
        || s.contains('&')
        || s.contains('*')
        || s.contains('!')
        || s.contains('|')
        || s.contains('>')
        || s.contains('%')
        || s.contains('@')
        || s.contains('`')
        || s.contains(',')
        || s.contains('?')
        || s.starts_with(' ')
        || s.ends_with(' ')
        || s.starts_with('-')
        || s == "true"
        || s == "false"
        || s == "null"
        || s == "~"
        || s.parse::<f64>().is_ok()
        || looks_like_yaml_int(s);

    if needs_quoting {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
#[path = "yaml_emitter_tests.rs"]
mod tests;
