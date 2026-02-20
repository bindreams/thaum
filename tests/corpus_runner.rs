use std::path::{Path, PathBuf};

use libtest_mimic::{Arguments, Failed, Trial};
use serde::Deserialize;
use yaml_rust2::Yaml;

// ---------------------------------------------------------------------------
// Test spec schema
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TestSpec {
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    dialect: String,
    source: Option<String>,

    #[serde(rename = "is-valid", default = "default_true")]
    is_valid: bool,
    error_contains: Option<String>,

    // ast is parsed separately from the raw YAML header using yaml_rust2
    // to avoid version mismatches with serde_yaml2's wrapper type.

    status: Option<i32>,
    stdout: Option<OutputMatcher>,
    stderr: Option<OutputMatcher>,
}

fn default_true() -> bool { true }

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
                        return Err(format!(
                            "{field_name} does not contain {:?}:\n  actual: {:?}",
                            substr, actual
                        )
                        .into());
                    }
                }
                if let Some(re_str) = &pat.regex {
                    let re = regex::Regex::new(re_str).map_err(|e| {
                        format!("{field_name} invalid regex {:?}: {}", re_str, e)
                    })?;
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

// ---------------------------------------------------------------------------
// YAML subset matching (using yaml_rust2::Yaml)
// ---------------------------------------------------------------------------

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
                let act_val = act_map.get(key).ok_or_else(|| {
                    format!("{}: key not found in actual AST", child_path)
                })?;
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
            if expected == actual {
                Ok(())
            } else if scalars_equivalent(expected, actual) {
                Ok(())
            } else {
                Err(format!(
                    "{}: expected {:?}, got {:?}",
                    path, expected, actual
                ))
            }
        }
    }
}

/// Parse a YAML string into a yaml_rust2::Yaml value (first document).
fn parse_yaml(s: &str) -> Result<Yaml, String> {
    let docs = yaml_rust2::YamlLoader::load_from_str(s)
        .map_err(|e| format!("YAML parse error: {}", e))?;
    docs.into_iter()
        .next()
        .ok_or_else(|| "empty YAML document".to_string())
}

// ---------------------------------------------------------------------------
// File parsing
// ---------------------------------------------------------------------------

struct ParsedTestFile {
    spec: TestSpec,
    ast: Option<Yaml>,
    shell_input: String,
}

fn parse_test_file(path: &Path) -> Result<ParsedTestFile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

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

    let spec: TestSpec = serde_yaml2::from_str(yaml_header)
        .map_err(|e| format!("{}: invalid YAML header: {}", path.display(), e))?;

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
        Yaml::Hash(ref map) => {
            Ok(map.get(&Yaml::String("ast".to_string())).cloned())
        }
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Test execution
// ---------------------------------------------------------------------------

fn run_test(parsed: &ParsedTestFile) -> Result<(), Failed> {
    let spec = &parsed.spec;
    let input = &parsed.shell_input;
    let dialect = spec.dialect().map_err(|e| e.to_string())?;

    // 1. Parse
    let parse_result = thaum::parse_with(input, dialect);

    if spec.is_valid {
        let program = parse_result.map_err(|e| {
            format!("expected parse: ok, but got error: {}", e)
        })?;

        // 2. AST assertion (optional)
        if let Some(expected_yaml) = &parsed.ast {
            let mapper = thaum::format::SourceMapper::new(input);
            let writer = thaum::format::YamlWriter::new_verbose(&mapper, "<test>");
            let actual_yaml_str = writer.write_program(&program);
            let actual_yaml = parse_yaml(&actual_yaml_str).map_err(|e| {
                format!(
                    "failed to re-parse verbose YAML: {}\n---\n{}",
                    e, actual_yaml_str
                )
            })?;
            yaml_is_subset(expected_yaml, &actual_yaml, "").map_err(|msg| {
                format!(
                    "AST mismatch: {}\n\nActual verbose YAML:\n{}",
                    msg, actual_yaml_str
                )
            })?;
        }

        // 3. Execution assertions (optional)
        if spec.status.is_some() || spec.stdout.is_some() || spec.stderr.is_some() {
            let mut executor = thaum::exec::Executor::new();
            let mut captured = thaum::exec::CapturedIo::new();
            let exit_code = match executor.execute(&program, &mut captured.context()) {
                Ok(code) => code,
                Err(thaum::exec::ExecError::ExitRequested(code)) => code,
                Err(thaum::exec::ExecError::CommandNotFound(_)) => 127,
                Err(e) => {
                    return Err(format!("execution error: {}", e).into());
                }
            };

            if let Some(expected_status) = spec.status {
                if exit_code != expected_status {
                    return Err(format!(
                        "status mismatch: expected {}, got {}",
                        expected_status, exit_code
                    )
                    .into());
                }
            }

            if let Some(ref stdout_matcher) = spec.stdout {
                stdout_matcher.check(&captured.stdout_string(), "stdout")?;
            }
            if let Some(ref stderr_matcher) = spec.stderr {
                stderr_matcher.check(&captured.stderr_string(), "stderr")?;
            }
        }
    } else {
        let err = parse_result.err().ok_or_else(|| {
            "expected parse: error, but parsing succeeded".to_string()
        })?;
        if let Some(ref substr) = spec.error_contains {
            let msg = err.to_string();
            if !msg.contains(substr.as_str()) {
                return Err(format!(
                    "error message does not contain {:?}:\n  actual: {:?}",
                    substr, msg
                )
                .into());
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Test discovery and harness
// ---------------------------------------------------------------------------

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

            Some(Trial::test(display_name, move || run_test(&parsed)))
        })
        .collect();

    libtest_mimic::run(&args, tests).exit();
}
