//! Shared parser for `.sh.yaml` test/benchmark scripts.
//!
//! The `.sh.yaml` format has a YAML header (metadata, assertions, environment)
//! separated from the shell script body by a `---` line. This module provides
//! [`ShYaml`] — a single struct that captures all header fields — and loading
//! functions used by both corpus tests and benchmarks.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

// ShYaml struct =======================================================================================================

/// Parsed `.sh.yaml` file — shared across corpus tests and benchmarks.
///
/// Consumers use only the fields they need:
/// - Corpus tests: all fields (validation, execution, output matching)
/// - Benchmarks: `dialect`, `body`, `setup`, `environment`
#[derive(Deserialize)]
pub struct ShYaml {
    /// Test/script name. Defaults to the filename stem if absent in YAML header.
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_dialect")]
    pub dialect: String,

    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub disabled: Disabled,

    #[serde(rename = "is-valid", default = "default_true")]
    pub is_valid: bool,
    #[serde(default)]
    pub error_contains: Option<String>,
    #[serde(rename = "parse-error")]
    pub parse_error: Option<ParseErrorSpec>,

    /// Environment variables set before setup/execution. Values undergo
    /// shell-style expansion so `PATH: "/my/bin:$PATH"` works.
    #[serde(default)]
    pub environment: HashMap<String, String>,

    /// Setup script executed before the test. Runs as a file in a fresh temp
    /// directory (shebang selects interpreter). The same directory becomes cwd
    /// for the main test script.
    pub setup: Option<String>,

    pub status: Option<i32>,
    pub stdout: Option<OutputMatcher>,
    pub stderr: Option<OutputMatcher>,

    /// Shell script body (after `---` separator). Populated by [`ShYaml::load`],
    /// not deserialized from the YAML header.
    #[serde(skip)]
    pub body: String,

    /// Raw YAML header text. Consumers that need dynamic YAML access (e.g. AST
    /// subset matching) can re-parse this with `yaml_rust2::YamlLoader`.
    #[serde(skip)]
    pub raw_header: String,
}

fn default_true() -> bool {
    true
}

fn default_dialect() -> String {
    "bash".to_string()
}

// Loading =============================================================================================================

impl ShYaml {
    /// Load a single `.sh.yaml` file, splitting header from body at `---`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;

        let separator_pos = content
            .find("\n---\n")
            .or_else(|| content.find("\n---"))
            .ok_or_else(|| {
                format!(
                    "{}: missing `---` separator between YAML header and shell code",
                    path.display()
                )
            })?;

        let yaml_header = &content[..separator_pos];
        let shell_start = separator_pos + "\n---".len();
        let body = if content[shell_start..].starts_with('\n') {
            content[shell_start + 1..].to_string()
        } else {
            content[shell_start..].to_string()
        };

        let mut spec: ShYaml =
            serde_yaml2::from_str(yaml_header).map_err(|e| format!("{}: invalid YAML header: {e}", path.display()))?;
        spec.body = body;
        spec.raw_header = yaml_header.to_string();

        // Derive name from filename if not specified in YAML header.
        if spec.name.is_empty() {
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            spec.name = file_name
                .strip_suffix(".sh.yaml")
                .or_else(|| file_name.strip_suffix(".sh"))
                .unwrap_or(&file_name)
                .to_string();
        }

        Ok(spec)
    }

    /// Load all `.sh.yaml` files from a directory, sorted by filename.
    pub fn load_dir(dir: &Path) -> Result<Vec<Self>, String> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".sh.yaml"))
            .collect();
        entries.sort_by_key(|e| e.file_name());

        let mut results = Vec::new();
        for entry in &entries {
            results.push(Self::load(&entry.path())?);
        }
        Ok(results)
    }

    /// Parse the dialect string into a [`Dialect`](thaum::Dialect).
    pub fn dialect(&self) -> Result<thaum::Dialect, String> {
        match self.dialect.as_str() {
            "posix" => Ok(thaum::Dialect::Posix),
            "dash" => Ok(thaum::Dialect::Dash),
            "bash44" => Ok(thaum::Dialect::Bash44),
            "bash50" => Ok(thaum::Dialect::Bash50),
            "bash51" => Ok(thaum::Dialect::Bash51),
            "bash" => Ok(thaum::Dialect::Bash),
            other => Err(format!("unknown dialect: {other:?}")),
        }
    }
}

// Supporting types ====================================================================================================

/// Supports `disabled: true` or `disabled: { reason: "..." }`. Default: false.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Disabled {
    Bool(bool),
    WithReason { reason: String },
}

impl Default for Disabled {
    fn default() -> Self {
        Disabled::Bool(false)
    }
}

impl Disabled {
    pub fn is_disabled(&self) -> bool {
        match self {
            Disabled::Bool(b) => *b,
            Disabled::WithReason { .. } => true,
        }
    }
}

/// Spec for the `parse-error` YAML field.
///
/// ```yaml
/// parse-error: true                          # just assert parsing fails
/// parse-error: "missing 'then' to close 'if'"  # exact message match
/// parse-error: { contains: "unexpected token" } # substring match
/// parse-error: { regex: "missing '\\w+'" }      # regex match
/// parse-error: { contains: "}", at: "1:1" }     # message + location
/// ```
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ParseErrorSpec {
    Bool(bool),
    Exact(String),
    Pattern(ParseErrorPattern),
}

#[derive(Deserialize)]
pub struct ParseErrorPattern {
    pub contains: Option<String>,
    pub regex: Option<String>,
    pub at: Option<String>,
}

/// Matches expected output (stdout/stderr) against actual output.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum OutputMatcher {
    Exact(String),
    Pattern(OutputPattern),
}

#[derive(Deserialize)]
pub struct OutputPattern {
    pub regex: Option<String>,
    pub contains: Option<String>,
}

impl OutputMatcher {
    /// Check that `actual` matches this matcher, returning a descriptive error on mismatch.
    pub fn check(&self, actual: &str, field_name: &str) -> Result<(), String> {
        match self {
            OutputMatcher::Exact(expected) => {
                if actual != expected {
                    return Err(format!(
                        "{field_name} mismatch:\n  expected: {expected:?}\n  actual:   {actual:?}"
                    ));
                }
            }
            OutputMatcher::Pattern(pat) => {
                if let Some(substr) = &pat.contains {
                    if !actual.contains(substr.as_str()) {
                        return Err(format!(
                            "{field_name} does not contain {substr:?}:\n  actual: {actual:?}"
                        ));
                    }
                }
                if let Some(re_str) = &pat.regex {
                    let re =
                        regex::Regex::new(re_str).map_err(|e| format!("{field_name} invalid regex {re_str:?}: {e}"))?;
                    if !re.is_match(actual) {
                        return Err(format!(
                            "{field_name} does not match regex {re_str:?}:\n  actual: {actual:?}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
