//! Callgrind smoke tests for the benchmark infrastructure.

use std::path::PathBuf;

use testutil::requires;

use super::preconditions;

fn scripts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/scripts")
}

#[requires(preconditions::valgrind, preconditions::thaum)]
fn callgrind_trivial_lex() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let script_path = scripts_dir().join("trivial.sh.yaml");
    let content = std::fs::read_to_string(&script_path).unwrap();
    let body = content.split("\n---\n").nth(1).unwrap();

    let tmp = std::env::temp_dir().join("thaum-bench-smoke-test");
    std::fs::create_dir_all(&tmp).unwrap();
    let out_file = tmp.join("trivial.lex.callgrind.out");

    let mut child = Command::new("valgrind")
        .args(["--tool=callgrind", "--error-exitcode=0"])
        .arg(format!("--callgrind-out-file={}", out_file.display()))
        .arg(preconditions::thaum_binary_path())
        .args(["--quiet", "lex", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn valgrind");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(body.as_bytes()).unwrap();
    }

    let status = child.wait().expect("failed to wait for valgrind");
    assert!(status.success(), "valgrind failed");

    let text = std::fs::read_to_string(&out_file).expect("cannot read callgrind output");
    let metrics = thaum::callgrind_parser::parse(&text).expect("cannot parse callgrind output");

    assert!(
        metrics.ir > 0,
        "instruction count should be positive, got {}",
        metrics.ir
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[requires(preconditions::valgrind, preconditions::thaum)]
fn callgrind_all_stages() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let script_path = scripts_dir().join("trivial.sh.yaml");
    let content = std::fs::read_to_string(&script_path).unwrap();
    let body = content.split("\n---\n").nth(1).unwrap();

    let tmp = std::env::temp_dir().join("thaum-bench-smoke-all-stages");
    std::fs::create_dir_all(&tmp).unwrap();

    for subcmd in &["lex", "parse", "exec"] {
        let out_file = tmp.join(format!("trivial.{subcmd}.callgrind.out"));

        let mut child = Command::new("valgrind")
            .args(["--tool=callgrind", "--error-exitcode=0"])
            .arg(format!("--callgrind-out-file={}", out_file.display()))
            .arg(preconditions::thaum_binary_path())
            .args(["--quiet", subcmd, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(body.as_bytes()).unwrap();
        }

        let status = child.wait().unwrap();
        assert!(status.success(), "valgrind failed for {subcmd}");

        let text = std::fs::read_to_string(&out_file).unwrap();
        let metrics = thaum::callgrind_parser::parse(&text).unwrap();
        assert!(metrics.ir > 0, "{subcmd}: instruction count should be positive");
    }

    let _ = std::fs::remove_dir_all(&tmp);
}
