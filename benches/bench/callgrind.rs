//! Callgrind backend: runs valgrind on the thaum binary, piping scripts via stdin.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};

use thaum::callgrind_parser;

use super::types::{BenchResult, Kind, Metric, Script, Stage, Value};

/// Map a Stage to the thaum CLI subcommand name.
fn stage_to_subcommand(stage: Stage) -> &'static str {
    match stage {
        Stage::Lex => "lex",
        Stage::Parse => "parse",
        Stage::Exec => "exec",
        Stage::Total => unreachable!("total is not a callgrind stage"),
    }
}

/// Map a Script dialect string to thaum CLI flags.
fn dialect_to_flags(dialect: &str) -> &'static [&'static str] {
    match dialect {
        "bash" => &["--bash"],
        "bash44" => &["--bash44"],
        "bash50" => &["--bash50"],
        "bash51" => &["--bash51"],
        _ => &[], // "posix" or unspecified => POSIX mode (default)
    }
}

/// Run callgrind benchmarks for all scripts and requested kinds.
pub fn run(thaum_binary: &std::path::Path, scripts: &[Script], kinds: &[Kind]) -> Vec<BenchResult> {
    // Determine which stages and metrics we need.
    let stages: HashSet<Stage> = kinds.iter().map(|k| k.stage).collect();
    let need_cache_sim = kinds.iter().any(|k| k.metric != Metric::Instructions);

    let tmp = std::env::temp_dir().join(format!(
        "thaum-bench-callgrind-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&tmp).expect("cannot create temp dir");

    let mut results: Vec<BenchResult> = Vec::new();

    for script in scripts {
        let script_content = std::fs::read_to_string(&script.path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", script.path.display()));

        let mut measurements: HashMap<Kind, Value> = HashMap::new();

        for &stage in &[Stage::Lex, Stage::Parse, Stage::Exec] {
            if !stages.contains(&stage) {
                continue;
            }

            let subcmd = stage_to_subcommand(stage);
            let out_file = tmp.join(format!("{}.{subcmd}.callgrind.out", script.name));

            let mut cmd = Command::new("valgrind");
            cmd.arg("--tool=callgrind")
                .arg("--error-exitcode=0")
                .arg(format!("--callgrind-out-file={}", out_file.display()));

            if need_cache_sim {
                cmd.arg("--cache-sim=yes");
            }

            cmd.arg(thaum_binary);
            cmd.arg("--quiet");
            for flag in dialect_to_flags(&script.dialect) {
                cmd.arg(flag);
            }
            cmd.arg(subcmd);
            cmd.arg("-"); // read from stdin

            cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());

            eprintln!("  callgrind: {} ({subcmd})", script.name);

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("    failed to run valgrind: {e}");
                    continue;
                }
            };

            // Pipe script content via stdin.
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(script_content.as_bytes());
                // stdin is dropped here, closing the pipe.
            }

            let status = child.wait().expect("failed to wait for valgrind");
            if !status.success() {
                eprintln!("    valgrind failed for {}/{subcmd}", script.name);
                continue;
            }

            let text = match std::fs::read_to_string(&out_file) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("    cannot read callgrind output: {e}");
                    continue;
                }
            };

            let metrics = match callgrind_parser::parse(&text) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("    cannot parse callgrind output: {e}");
                    continue;
                }
            };

            for kind in kinds {
                if kind.stage != stage {
                    continue;
                }
                if let Some(val) = metric_for(&metrics, kind.metric) {
                    measurements.insert(*kind, val);
                }
            }
        }

        if !measurements.is_empty() {
            results.push(BenchResult {
                name: script.name.clone(),
                measurements,
            });
        }
    }

    let _ = std::fs::remove_dir_all(&tmp);
    results
}

fn metric_for(m: &callgrind_parser::CallgrindMetrics, metric: Metric) -> Option<Value> {
    match metric {
        Metric::Instructions => Some(Value::Count(m.ir)),
        Metric::DataReads => Some(Value::Count(m.dr)),
        Metric::DataWrites => Some(Value::Count(m.dw)),
        Metric::L1Hits => Some(Value::Count(m.l1_hits())),
        Metric::LlHits => Some(Value::Count(m.ll_hits())),
        Metric::RamHits => Some(Value::Count(m.ram_hits())),
        Metric::EstCycles => Some(Value::Count(m.est_cycles())),
        Metric::Walltime => None,
    }
}

#[cfg(test)]
#[allow(dead_code)] // Bench target (harness=false) sees these as dead code; bin target runs them.
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn stage_to_subcommand_mapping() {
        assert_eq!(stage_to_subcommand(Stage::Lex), "lex");
        assert_eq!(stage_to_subcommand(Stage::Parse), "parse");
        assert_eq!(stage_to_subcommand(Stage::Exec), "exec");
    }

    #[test]
    fn dialect_to_flags_mapping() {
        assert_eq!(dialect_to_flags("bash"), &["--bash"]);
        assert_eq!(dialect_to_flags("bash44"), &["--bash44"]);
        assert_eq!(dialect_to_flags("posix"), &[] as &[&str]);
        assert_eq!(dialect_to_flags("unknown"), &[] as &[&str]);
    }

    fn has_valgrind() -> bool {
        Command::new("valgrind")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn thaum_binary() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/thaum");
        assert!(
            path.exists(),
            "thaum binary not found at {}. Run: cargo test --features cli,bench",
            path.display()
        );
        path
    }

    fn scripts_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/scripts")
    }

    #[test]
    fn callgrind_trivial_lex() {
        if !has_valgrind() {
            eprintln!("skipping: valgrind not found");
            return;
        }

        let scripts = super::super::types::load_scripts(&scripts_dir().join("trivial.sh.yaml"));
        assert_eq!(scripts.len(), 1);

        let kinds = vec![Kind {
            stage: Stage::Lex,
            metric: Metric::Instructions,
        }];
        let results = run(&thaum_binary(), &scripts, &kinds);

        assert_eq!(results.len(), 1, "expected one result");
        assert_eq!(results[0].name, "trivial");
        let val = results[0]
            .measurements
            .get(&kinds[0])
            .expect("missing lex.instructions");
        match val {
            Value::Count(n) => assert!(*n > 0, "instruction count should be positive"),
            _ => panic!("expected Count, got Time"),
        }
    }

    #[test]
    fn callgrind_all_stages() {
        if !has_valgrind() {
            eprintln!("skipping: valgrind not found");
            return;
        }

        let scripts = super::super::types::load_scripts(&scripts_dir().join("trivial.sh.yaml"));
        let kinds = vec![
            Kind {
                stage: Stage::Lex,
                metric: Metric::Instructions,
            },
            Kind {
                stage: Stage::Parse,
                metric: Metric::Instructions,
            },
            Kind {
                stage: Stage::Exec,
                metric: Metric::Instructions,
            },
        ];
        let results = run(&thaum_binary(), &scripts, &kinds);

        assert_eq!(results.len(), 1);
        for kind in &kinds {
            let val = results[0]
                .measurements
                .get(kind)
                .unwrap_or_else(|| panic!("missing {}", kind));
            match val {
                Value::Count(n) => assert!(*n > 0, "{} should be positive", kind),
                _ => panic!("expected Count for {}", kind),
            }
        }
    }
}
