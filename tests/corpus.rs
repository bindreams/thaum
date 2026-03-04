mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use libtest_mimic::Failed;
use serde::Deserialize;
use thaum::exec::{CapturedIo, ExecError, Executor};
use yaml_rust2::Yaml;

/// Whether --no-sandbox was passed on the command line.
static NO_SANDBOX: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// Test spec schema ----------------------------------------------------------------------------------------------------

/// Supports `disabled: true` or `disabled: { reason: "..." }`. Default: false.
#[derive(Deserialize)]
#[serde(untagged)]
enum Disabled {
    Bool(bool),
    WithReason { reason: String },
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

    /// Environment variables set before setup/execution.  Values undergo
    /// shell-style expansion so `PATH: "/my/bin:$PATH"` works.
    #[serde(default)]
    environment: std::collections::HashMap<String, String>,

    /// Setup script executed before the test. Runs as a file in a fresh temp
    /// directory (shebang selects interpreter). The same directory becomes cwd
    /// for the main test script.
    setup: Option<String>,

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
            "dash" => Ok(thaum::Dialect::Dash),
            "bash44" => Ok(thaum::Dialect::Bash44),
            "bash50" => Ok(thaum::Dialect::Bash50),
            "bash51" => Ok(thaum::Dialect::Bash51),
            "bash" => Ok(thaum::Dialect::Bash),
            other => Err(format!("unknown dialect: {other:?}")),
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
                    return Err(
                        format!("{field_name} mismatch:\n  expected: {expected:?}\n  actual:   {actual:?}").into(),
                    );
                }
            }
            OutputMatcher::Pattern(pat) => {
                if let Some(substr) = &pat.contains {
                    if !actual.contains(substr.as_str()) {
                        return Err(format!("{field_name} does not contain {substr:?}:\n  actual: {actual:?}").into());
                    }
                }
                if let Some(re_str) = &pat.regex {
                    let re =
                        regex::Regex::new(re_str).map_err(|e| format!("{field_name} invalid regex {re_str:?}: {e}"))?;
                    if !re.is_match(actual) {
                        return Err(
                            format!("{field_name} does not match regex {re_str:?}:\n  actual: {actual:?}").into(),
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

// YAML subset matching (using yaml_rust2::Yaml) -----------------------------------------------------------------------

/// Compare YAML scalars that may differ in type due to YAML's implicit typing.
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
                    _ => format!("{key:?}"),
                };
                let child_path = if path.is_empty() {
                    key_str.clone()
                } else {
                    format!("{path}.{key_str}")
                };
                let act_val = act_map
                    .get(key)
                    .ok_or_else(|| format!("{child_path}: key not found in actual AST"))?;
                yaml_is_subset(exp_val, act_val, &child_path)?;
            }
            Ok(())
        }
        (Yaml::Array(exp_seq), Yaml::Array(act_seq)) => {
            for (i, exp_item) in exp_seq.iter().enumerate() {
                let child_path = format!("{path}[{i}]");
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
                Err(format!("{path}: expected {expected:?}, got {actual:?}"))
            }
        }
    }
}

/// Parse a YAML string into a yaml_rust2::Yaml value (first document).
fn parse_yaml(s: &str) -> Result<Yaml, String> {
    let docs = yaml_rust2::YamlLoader::load_from_str(s).map_err(|e| format!("YAML parse error: {e}"))?;
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
                return Err(format!("parse-error mismatch:\n  expected: {expected:?}\n  actual:   {msg:?}").into());
            }
        }
        ParseErrorSpec::Pattern(pat) => {
            if let Some(substr) = &pat.contains {
                if !msg.contains(substr.as_str()) {
                    return Err(format!("parse-error does not contain {substr:?}:\n  actual: {msg:?}").into());
                }
            }
            if let Some(re_str) = &pat.regex {
                let re = regex::Regex::new(re_str).map_err(|e| format!("parse-error invalid regex {re_str:?}: {e}"))?;
                if !re.is_match(&msg) {
                    return Err(format!("parse-error does not match regex {re_str:?}:\n  actual: {msg:?}").into());
                }
            }
            if let Some(expected_at) = &pat.at {
                let span = err.span().ok_or_else(|| {
                    format!("parse-error `at: {expected_at:?}` specified but error has no span:\n  error: {msg:?}")
                })?;
                let (line, col) = byte_offset_to_line_col(source, span.start.0);
                let actual_at = format!("{line}:{col}");
                if actual_at != *expected_at {
                    return Err(format!(
                        "parse-error location mismatch:\n  expected: {expected_at:?}\n  actual:   {actual_at:?}"
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}

// Native execution ====================================================================================================

/// Execute a test script natively using the thaum Rust API.
///
/// Creates a temp directory (auto-cleaned on drop), sets up environment
/// variables, runs setup scripts, then executes via `Executor` with `CapturedIo`.
fn run_exec_native(
    spec: &TestSpec,
    input: &str,
    dialect: thaum::Dialect,
) -> Result<common::docker::ExecResult, Failed> {
    let dir = tempfile::tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let saved_cwd = std::env::current_dir().ok();
    std::env::set_current_dir(dir.path()).map_err(|e| format!("chdir: {e}"))?;

    // Set environment variables.
    let mut saved_env = Vec::new();
    for (key, value) in &spec.environment {
        saved_env.push((key.clone(), std::env::var(key).ok()));
        std::env::set_var(key, value);
    }

    // Prepend workdir to PATH so setup-created helpers are found.
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old_path}", dir.path().display()));

    // Write and execute setup script if provided.
    if let Some(setup_script) = &spec.setup {
        let setup_path = dir.path().join(".setup");
        std::fs::write(&setup_path, setup_script).map_err(|e| format!("write setup: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&setup_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("chmod setup: {e}"))?;
        }
        let status = Command::new(&setup_path)
            .current_dir(dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("run setup: {e}"))?;
        if !status.success() {
            return Err(format!("setup script failed with {status}").into());
        }
    }

    // Parse and execute.
    let program = thaum::parse_with(input, dialect).map_err(|e| format!("parse: {e}"))?;
    let options = dialect.options();
    let mut exec = Executor::with_options(options);
    let _ = exec
        .env_mut()
        .set_var("PATH", &std::env::var("PATH").unwrap_or_default());

    let mut io = CapturedIo::new();
    let exit_code = match exec.execute(&program, &mut io.context()) {
        Ok(status) => status,
        Err(ExecError::ExitRequested(code)) => code,
        Err(_) => 127,
    };

    let result = common::docker::ExecResult {
        stdout: io.stdout_string(),
        stderr: io.stderr_string(),
        exit_code,
    };

    // Restore environment.
    for (key, old_val) in saved_env {
        match old_val {
            Some(v) => std::env::set_var(&key, v),
            None => std::env::remove_var(&key),
        }
    }
    std::env::set_var("PATH", old_path);
    if let Some(cwd) = saved_cwd {
        let _ = std::env::set_current_dir(cwd);
    }

    Ok(result)
}

// Docker proxy execution ==============================================================================================

/// Execute a corpus test inside Docker by running the corpus binary with --no-sandbox.
///
/// Invokes the compiled corpus binary inside the container with `--format json`
/// and `--exact` to run a single test. Parses the libtest JSON output to
/// determine pass/fail.
fn run_exec_docker(container_id: &str, test_name: &str) -> Result<common::docker::ExecResult, Failed> {
    let output = Command::new("docker")
        .args([
            "exec",
            container_id,
            "/usr/local/bin/corpus-test",
            "--no-sandbox",
            "--format",
            "json",
            "--exact",
            test_name,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("docker exec failed: {e}"))?;

    // Parse libtest JSON events from stdout to find the test result.
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);

    for line in stdout_str.lines() {
        // Look for test result event: {"type":"test","name":"...","event":"ok"|"failed",...}
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
            if event.get("type").and_then(|v| v.as_str()) == Some("test")
                && event.get("name").is_some()
                && event.get("event").is_some()
            {
                let event_type = event["event"].as_str().unwrap_or("");
                match event_type {
                    "ok" => {
                        return Ok(common::docker::ExecResult {
                            stdout: String::new(),
                            stderr: String::new(),
                            exit_code: 0,
                        });
                    }
                    "failed" => {
                        let message = event
                            .get("stdout")
                            .and_then(|v| v.as_str())
                            .unwrap_or("test failed (no details)");
                        return Err(message.to_string().into());
                    }
                    "ignored" => {
                        return Ok(common::docker::ExecResult {
                            stdout: String::new(),
                            stderr: String::new(),
                            exit_code: 0,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    // No test result event found — the binary likely failed to start.
    Err(format!("corpus binary produced no test result\nstdout: {stdout_str}\nstderr: {stderr_str}").into())
}

// Test execution ======================================================================================================

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
        let program = parse_result.map_err(|e| format!("expected parse: ok, but got error: {e}"))?;

        // 2. AST assertion (optional)
        if let Some(expected_yaml) = &parsed.ast {
            let mapper = thaum::format::SourceMapper::new(input);
            let writer = thaum::format::YamlWriter::new_verbose(&mapper, "<test>");
            let actual_yaml_str = writer.write_program(&program);
            let actual_yaml = parse_yaml(&actual_yaml_str)
                .map_err(|e| format!("failed to re-parse verbose YAML: {e}\n---\n{actual_yaml_str}"))?;
            yaml_is_subset(expected_yaml, &actual_yaml, "")
                .map_err(|msg| format!("AST mismatch: {msg}\n\nActual verbose YAML:\n{actual_yaml_str}"))?;
        }

        // 3. Execution assertions (optional, --no-sandbox native mode only)
        if spec.status.is_some() || spec.stdout.is_some() || spec.stderr.is_some() {
            let result = run_exec_native(spec, input, dialect)?;
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
                return Err(format!("error message does not contain {substr:?}:\n  actual: {msg:?}").into());
            }
        }
    }

    // 4. POSIX-rejection check (optional, controlled by POSIX_REJECTION_CHECK=1).
    if matches!(spec.dialect.as_str(), "bash" | "bash44" | "bash50" | "bash51")
        && spec.is_valid
        && std::env::var("POSIX_REJECTION_CHECK").is_ok()
    {
        if let Ok(posix_program) = thaum::parse_with(input, thaum::Dialect::Posix) {
            let posix_options = thaum::Dialect::Posix.options();
            let mut posix_exec = thaum::exec::Executor::with_options(posix_options);
            let _ = posix_exec.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
            let mut posix_io = thaum::exec::CapturedIo::new();
            let posix_result = posix_exec.execute(&posix_program, &mut posix_io.context());
            if let Ok(status) = posix_result {
                let stdout_matches = match &spec.stdout {
                    Some(matcher) => matcher.check(&posix_io.stdout_string(), "stdout").is_ok(),
                    None => true,
                };
                let status_matches = match spec.status {
                    Some(expected) => status == expected,
                    None => status == 0,
                };
                if status_matches && stdout_matches {
                    eprintln!(
                        "POSIX-COMPAT: bash test '{}' also passes in POSIX mode (status + stdout match)",
                        spec.name
                    );
                }
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
    let no_sandbox = std::env::args().any(|a| a == "--no-sandbox");
    NO_SANDBOX.store(no_sandbox, std::sync::atomic::Ordering::Relaxed);

    // Check if corpus execution is available. For Docker mode, this only runs
    // `docker info` (fast, idempotent).
    let exec_available = no_sandbox
        || skuld::collect_fixture_requires(&["corpus_sandbox"])
            .iter()
            .all(|check| check().is_ok());

    // Eagerly build the Docker image and start the container before tests run.
    // This avoids per-test timeout issues (Docker build can take minutes).
    if exec_available && !no_sandbox {
        skuld::warm_up("corpus_sandbox");
    }

    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let files = discover_corpus_files(&corpus_dir);

    let mut runner = skuld::TestRunner::new();
    runner.strip_args(&["--no-sandbox"]);

    for path in files {
        let rel = path
            .strip_prefix(&corpus_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace(".sh.yaml", "")
            .replace('\\', "/");

        let parsed = match parse_test_file(&path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("warning: skipping {rel}: {e}");
                continue;
            }
        };

        let test_name = parsed.spec.name.clone();
        let disabled = parsed.spec.disabled.is_disabled();
        let display_name = match &parsed.spec.disabled {
            Disabled::WithReason { reason } => {
                format!("{rel} ({test_name}) [disabled: {reason}]")
            }
            _ => format!("{rel} ({test_name})"),
        };

        let has_exec = parsed.spec.status.is_some() || parsed.spec.stdout.is_some() || parsed.spec.stderr.is_some();
        let labels: Vec<&str> = if has_exec {
            vec!["corpus", "lex", "parse", "exec"]
        } else {
            vec!["corpus", "lex", "parse"]
        };

        // Disable exec tests when Docker is unavailable (unless --no-sandbox).
        let ignored = disabled || (has_exec && !exec_available);

        if has_exec && !no_sandbox && !disabled {
            // Docker mode: delegate the entire test to the corpus binary inside Docker.
            let display_name_for_docker = display_name.clone();
            runner.add(display_name, &labels, ignored, move || {
                let sandbox: &common::docker::CorpusSandbox = skuld::fixture("corpus_sandbox");
                if let Err(e) = run_exec_docker(&sandbox.container_id, &display_name_for_docker) {
                    panic!("{}", e.message().unwrap_or("test failed"));
                }
            });
        } else {
            // Native mode (--no-sandbox) or parse-only tests: run locally.
            runner.add(display_name, &labels, ignored, move || {
                if let Err(e) = run_test(&parsed) {
                    panic!("{}", e.message().unwrap_or("test failed"));
                }
            });
        }
    }

    // Process fixtures (corpus_image, corpus_sandbox) are cleaned up
    // automatically inside run_tests() — image removed, container killed.
    runner.run_tests().exit();
}
