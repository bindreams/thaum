//! Callgrind smoke tests for the benchmark infrastructure.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::preconditions;

testutil::default_labels!(bench);

fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/scripts")
}

/// Read a `.sh.yaml` benchmark script and return the shell body (after `---`).
fn read_script_body(name: &str) -> String {
    let path = scripts_dir().join(name);
    let content = std::fs::read_to_string(&path).unwrap();
    content
        .split("\n---\n")
        .nth(1)
        .unwrap_or_else(|| panic!("{name}: missing --- separator"))
        .to_string()
}

/// Run a single callgrind invocation and return the parsed metrics.
fn run_callgrind(subcmd: &str, script_body: &str) -> thaum::callgrind_parser::CallgrindMetrics {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let out_file = tmp.path().join(format!("{subcmd}.callgrind.out"));

    let mut child = Command::new("valgrind")
        .args(["--tool=callgrind", "--error-exitcode=0"])
        .arg(format!("--callgrind-out-file={}", out_file.display()))
        .arg(preconditions::thaum_binary_path())
        .args(["--quiet", subcmd, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn valgrind");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(script_body.as_bytes()).unwrap();
    }

    let status = child.wait().expect("failed to wait for valgrind");
    assert!(status.success(), "valgrind failed for {subcmd}");

    let text = std::fs::read_to_string(&out_file).expect("cannot read callgrind output");
    thaum::callgrind_parser::parse(&text).expect("cannot parse callgrind output")
}

#[testutil::test(requires = [preconditions::valgrind, preconditions::thaum])]
fn callgrind_trivial_lex() {
    let body = read_script_body("trivial.sh.yaml");
    let metrics = run_callgrind("lex", &body);

    assert!(
        metrics.ir > 0,
        "instruction count should be positive, got {}",
        metrics.ir
    );
}

#[testutil::test(requires = [preconditions::valgrind, preconditions::thaum])]
fn callgrind_all_stages() {
    let body = read_script_body("trivial.sh.yaml");

    for subcmd in &["lex", "parse", "exec"] {
        let metrics = run_callgrind(subcmd, &body);
        assert!(metrics.ir > 0, "{subcmd}: instruction count should be positive");
    }
}
