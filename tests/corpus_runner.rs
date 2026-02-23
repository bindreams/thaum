mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use libtest_mimic::{Arguments, Failed, Trial};
use serde::Deserialize;
use yaml_rust2::Yaml;

/// Checked once at startup; cached to avoid repeated docker calls.
static DOCKER_AVAILABLE: AtomicBool = AtomicBool::new(false);

// Test spec schema ----------------------------------------------------------------------------------------------------

/// Supports `disabled: true` or `disabled: { reason: "..." }`. Default: false.
#[derive(Deserialize)]
#[serde(untagged)]
enum Disabled {
    Bool(bool),
    WithReason {
        #[allow(dead_code)]
        reason: String,
    },
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
enum ParseErrorSpec {
    Bool(bool),
    Exact(String),
    Pattern(ParseErrorPattern),
}

#[derive(Deserialize)]
struct ParseErrorPattern {
    contains: Option<String>,
    regex: Option<String>,
    at: Option<String>,
}

impl Default for Disabled {
    fn default() -> Self {
        Disabled::Bool(false)
    }
}

impl Disabled {
    fn is_disabled(&self) -> bool {
        match self {
            Disabled::Bool(b) => *b,
            Disabled::WithReason { .. } => true,
        }
    }
}

#[derive(Deserialize)]
struct TestSpec {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    tags: Vec<String>,
    dialect: String,
    #[allow(dead_code)]
    source: Option<String>,

    #[serde(default)]
    disabled: Disabled,

    #[serde(rename = "is-valid", default = "default_true")]
    is_valid: bool,
    error_contains: Option<String>,

    #[serde(rename = "parse-error")]
    parse_error: Option<ParseErrorSpec>,

    // ast is parsed separately from the raw YAML header using yaml_rust2
    // to avoid version mismatches with serde_yaml2's wrapper type.
    status: Option<i32>,
    stdout: Option<OutputMatcher>,
    stderr: Option<OutputMatcher>,
}

fn default_true() -> bool {
    true
}

impl TestSpec {
    fn dialect(&self) -> Result<thaum::Dialect, String> {
        match self.dialect.as_str() {
            "posix" => Ok(thaum::Dialect::Posix),
            "bash" => Ok(thaum::Dialect::Bash),
            other => Err(format!("unknown dialect: {:?}", other)),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OutputMatcher {
    Exact(String),
    Pattern(OutputPattern),
}

#[derive(Deserialize)]
struct OutputPattern {
    regex: Option<String>,
    contains: Option<String>,
}

impl OutputMatcher {
    fn check(&self, actual: &str, field_name: &str) -> Result<(), Failed> {
        match self {
            OutputMatcher::Exact(expected) => {
                if actual != expected {
                    return Err(format!(
                        "{field_name} mismatch:\n  expected: {:?}\n  actual:   {:?}",
                        expected, actual
                    )
                    .into());
                }
            }
            OutputMatcher::Pattern(pat) => {
                if let Some(substr) = &pat.contains {
                    if !actual.contains(substr.as_str()) {
                        return Err(
                            format!("{field_name} does not contain {:?}:\n  actual: {:?}", substr, actual).into(),
                        );
                    }
                }
                if let Some(re_str) = &pat.regex {
                    let re = regex::Regex::new(re_str)
                        .map_err(|e| format!("{field_name} invalid regex {:?}: {}", re_str, e))?;
                    if !re.is_match(actual) {
                        return Err(format!(
                            "{field_name} does not match regex {:?}:\n  actual: {:?}",
                            re_str, actual
                        )
                        .into());
                    }
                }
            }
        }
        Ok(())
    }
}

// YAML subset matching (using yaml_rust2::Yaml) -----------------------------------------------------------------------

/// Compare YAML scalars that may differ in type due to YAML's implicit typing.
/// E.g., `9` (Integer) should equal `"9"` (String) since our YAML emitter
/// now quotes values but test authors may write unquoted numbers.
fn scalars_equivalent(a: &Yaml, b: &Yaml) -> bool {
    fn to_str(v: &Yaml) -> Option<String> {
        match v {
            Yaml::String(s) => Some(s.clone()),
            Yaml::Integer(n) => Some(n.to_string()),
            Yaml::Real(s) => Some(s.clone()),
            Yaml::Boolean(b) => Some(b.to_string()),
            Yaml::Null => Some("null".to_string()),
            _ => None,
        }
    }
    match (to_str(a), to_str(b)) {
        (Some(sa), Some(sb)) => sa == sb,
        _ => false,
    }
}

fn yaml_is_subset(expected: &Yaml, actual: &Yaml, path: &str) -> Result<(), String> {
    match (expected, actual) {
        (Yaml::Hash(exp_map), Yaml::Hash(act_map)) => {
            for (key, exp_val) in exp_map {
                let key_str = match key {
                    Yaml::String(s) => s.clone(),
                    _ => format!("{:?}", key),
                };
                let child_path = if path.is_empty() {
                    key_str.clone()
                } else {
                    format!("{}.{}", path, key_str)
                };
                let act_val = act_map
                    .get(key)
                    .ok_or_else(|| format!("{}: key not found in actual AST", child_path))?;
                yaml_is_subset(exp_val, act_val, &child_path)?;
            }
            Ok(())
        }
        (Yaml::Array(exp_seq), Yaml::Array(act_seq)) => {
            for (i, exp_item) in exp_seq.iter().enumerate() {
                let child_path = format!("{}[{}]", path, i);
                let act_item = act_seq.get(i).ok_or_else(|| {
                    format!(
                        "{}: expected element at index {} but actual has only {} elements",
                        child_path,
                        i,
                        act_seq.len()
                    )
                })?;
                yaml_is_subset(exp_item, act_item, &child_path)?;
            }
            Ok(())
        }
        _ => {
            if expected == actual || scalars_equivalent(expected, actual) {
                Ok(())
            } else {
                Err(format!("{}: expected {:?}, got {:?}", path, expected, actual))
            }
        }
    }
}

/// Parse a YAML string into a yaml_rust2::Yaml value (first document).
fn parse_yaml(s: &str) -> Result<Yaml, String> {
    let docs = yaml_rust2::YamlLoader::load_from_str(s).map_err(|e| format!("YAML parse error: {}", e))?;
    docs.into_iter().next().ok_or_else(|| "empty YAML document".to_string())
}

// File parsing --------------------------------------------------------------------------------------------------------

struct ParsedTestFile {
    spec: TestSpec,
    ast: Option<Yaml>,
    shell_input: String,
}

fn parse_test_file(path: &Path) -> Result<ParsedTestFile, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

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
    // Skip past "\n---\n" (or "\n---" at end of file)
    let shell_start = separator_pos + "\n---".len();
    let shell_input = if content[shell_start..].starts_with('\n') {
        content[shell_start + 1..].to_string()
    } else {
        content[shell_start..].to_string()
    };

    let spec: TestSpec =
        serde_yaml2::from_str(yaml_header).map_err(|e| format!("{}: invalid YAML header: {}", path.display(), e))?;

    // Parse `ast:` field separately using yaml_rust2 (YAML 1.2) to get
    // a dynamic Yaml value for subset matching.
    let ast = extract_ast_field(yaml_header)?;

    Ok(ParsedTestFile { spec, ast, shell_input })
}

/// Extract the `ast:` field from the YAML header as a yaml_rust2::Yaml value.
/// Returns None if the field is absent.
fn extract_ast_field(yaml_header: &str) -> Result<Option<Yaml>, String> {
    let doc = parse_yaml(yaml_header)?;
    match doc {
        Yaml::Hash(ref map) => Ok(map.get(&Yaml::String("ast".to_string())).cloned()),
        _ => Ok(None),
    }
}

// Test execution ------------------------------------------------------------------------------------------------------

/// Convert a byte offset in `source` to a 1-based (line, col) pair.
fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i == offset {
            return (line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    // offset is at or past the end of the string
    (line, col)
}

fn check_parse_error(spec: &ParseErrorSpec, err: &thaum::ParseError, source: &str) -> Result<(), Failed> {
    let msg = err.to_string();
    match spec {
        ParseErrorSpec::Bool(true) => { /* just assert failure — already done by caller */ }
        ParseErrorSpec::Bool(false) => {
            return Err("parse-error: false is not meaningful; remove the field".into());
        }
        ParseErrorSpec::Exact(expected) => {
            if msg != *expected {
                return Err(format!(
                    "parse-error mismatch:\n  expected: {:?}\n  actual:   {:?}",
                    expected, msg
                )
                .into());
            }
        }
        ParseErrorSpec::Pattern(pat) => {
            if let Some(substr) = &pat.contains {
                if !msg.contains(substr.as_str()) {
                    return Err(format!("parse-error does not contain {:?}:\n  actual: {:?}", substr, msg).into());
                }
            }
            if let Some(re_str) = &pat.regex {
                let re =
                    regex::Regex::new(re_str).map_err(|e| format!("parse-error invalid regex {:?}: {}", re_str, e))?;
                if !re.is_match(&msg) {
                    return Err(format!("parse-error does not match regex {:?}:\n  actual: {:?}", re_str, msg).into());
                }
            }
            if let Some(expected_at) = &pat.at {
                let span = err.span().ok_or_else(|| {
                    format!(
                        "parse-error `at: {:?}` specified but error has no span:\n  error: {:?}",
                        expected_at, msg
                    )
                })?;
                let (line, col) = byte_offset_to_line_col(source, span.start.0);
                let actual_at = format!("{}:{}", line, col);
                if actual_at != *expected_at {
                    return Err(format!(
                        "parse-error location mismatch:\n  expected: {:?}\n  actual:   {:?}",
                        expected_at, actual_at
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}

fn run_test(parsed: &ParsedTestFile) -> Result<(), Failed> {
    let spec = &parsed.spec;
    let input = &parsed.shell_input;
    let dialect = spec.dialect().map_err(|e| e.to_string())?;

    // 1. Parse
    let parse_result = thaum::parse_with(input, dialect);

    // 1a. parse-error assertion (takes priority over is-valid)
    if let Some(ref error_spec) = spec.parse_error {
        let err = parse_result
            .err()
            .ok_or_else(|| "parse-error field present, but parsing succeeded".to_string())?;
        check_parse_error(error_spec, &err, input)?;
        return Ok(());
    }

    if spec.is_valid {
        let program = parse_result.map_err(|e| format!("expected parse: ok, but got error: {}", e))?;

        // 2. AST assertion (optional)
        if let Some(expected_yaml) = &parsed.ast {
            let mapper = thaum::format::SourceMapper::new(input);
            let writer = thaum::format::YamlWriter::new_verbose(&mapper, "<test>");
            let actual_yaml_str = writer.write_program(&program);
            let actual_yaml = parse_yaml(&actual_yaml_str)
                .map_err(|e| format!("failed to re-parse verbose YAML: {}\n---\n{}", e, actual_yaml_str))?;
            yaml_is_subset(expected_yaml, &actual_yaml, "")
                .map_err(|msg| format!("AST mismatch: {}\n\nActual verbose YAML:\n{}", msg, actual_yaml_str))?;
        }

        // 3. Execution assertions (optional, requires Docker)
        if spec.status.is_some() || spec.stdout.is_some() || spec.stderr.is_some() {
            if !DOCKER_AVAILABLE.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            let bash_flag = spec.dialect.as_str() == "bash";
            let result = common::docker::run_thaum_in_docker(input, bash_flag);

            if let Some(expected_status) = spec.status {
                if result.exit_code != expected_status {
                    return Err(format!(
                        "status mismatch: expected {}, got {}",
                        expected_status, result.exit_code
                    )
                    .into());
                }
            }

            if let Some(ref stdout_matcher) = spec.stdout {
                stdout_matcher.check(&result.stdout, "stdout")?;
            }
            if let Some(ref stderr_matcher) = spec.stderr {
                stderr_matcher.check(&result.stderr, "stderr")?;
            }
        }
    } else {
        let err = parse_result
            .err()
            .ok_or_else(|| "expected parse: error, but parsing succeeded".to_string())?;
        if let Some(ref substr) = spec.error_contains {
            let msg = err.to_string();
            if !msg.contains(substr.as_str()) {
                return Err(format!("error message does not contain {:?}:\n  actual: {:?}", substr, msg).into());
            }
        }
    }

    Ok(())
}

// Test discovery and harness ------------------------------------------------------------------------------------------

fn discover_corpus_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }
    collect_files_recursive(dir, &mut files);
    files.sort();
    files
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".sh.yaml") {
                    out.push(path);
                }
            }
        }
    }
}

fn main() {
    let args = Arguments::from_args();

    // Check Docker availability once at startup
    let docker_ok = common::docker::docker_image_available("thaum-corpus-exec");
    DOCKER_AVAILABLE.store(docker_ok, std::sync::atomic::Ordering::Relaxed);
    if !docker_ok {
        eprintln!("note: thaum-corpus-exec Docker image not found; execution assertions will be skipped");
        eprintln!("      run scripts/build-corpus-docker.sh to enable them");
    }

    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let files = discover_corpus_files(&corpus_dir);

    let tests: Vec<Trial> = files
        .into_iter()
        .filter_map(|path| {
            let rel = path
                .strip_prefix(&corpus_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace(".sh.yaml", "")
                .replace('\\', "/");

            let parsed = match parse_test_file(&path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("warning: skipping {}: {}", rel, e);
                    return None;
                }
            };

            let test_name = parsed.spec.name.clone();
            let display_name = format!("{} ({})", rel, test_name);
            let disabled = parsed.spec.disabled.is_disabled();

            Some(Trial::test(display_name, move || run_test(&parsed)).with_ignored_flag(disabled))
        })
        .collect();

    libtest_mimic::run(&args, tests).exit();
}
