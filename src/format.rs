//! AST formatting: YAML output and canonical shell source.
//!
//! Provides the YAML emitter, value model, and AST writer used by both the CLI
//! and the corpus test runner, plus a source writer for `declare -f` output.

/// Byte-offset to line/column mapper for source-location display.
pub mod source_map;
mod yaml_emitter;
/// Lightweight YAML data model (mapping, sequence, scalar, null).
pub mod yaml_value;
/// Converts AST nodes into `YamlValue` trees and emits YAML text.
pub mod yaml_writer;

pub use source_map::SourceMapper;
/// Converts AST back to canonical shell source (for `declare -f`).
pub mod source_writer;

pub use source_writer::SourceWriter;
pub use yaml_writer::YamlWriter;
