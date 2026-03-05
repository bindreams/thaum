//! Hyperfine backend: wall-clock comparison of thaum vs bash vs dash.

use std::collections::{HashMap, HashSet};
use std::process::Command;

use super::types::{BenchResult, Kind, Metric, Script, Stage, Value};

fn shells_for_dialect(dialect: &str) -> &[&str] {
    match dialect {
        "posix" | "dash" => &["bash", "dash"],
        _ => &["bash"],
    }
}

/// Run hyperfine benchmarks for the requested walltime stages.
pub fn run(scripts: &[Script], kinds: &[Kind]) -> Vec<BenchResult> {
    let stages: HashSet<Stage> = kinds
        .iter()
        .filter(|k| k.metric == Metric::Walltime)
        .map(|k| k.stage)
        .collect();

    if stages.is_empty() {
        return Vec::new();
    }

    let tmp = std::env::temp_dir().join("thaum-bench-hyperfine");
    std::fs::create_dir_all(&tmp).expect("cannot create temp dir");

    let mut results: Vec<BenchResult> = Vec::new();

    for script in scripts {
        let shells = shells_for_dialect(&script.dialect);
        let setup_dir = script.run_setup();
        let mut measurements: HashMap<Kind, Value> = HashMap::new();

        for &stage in &[Stage::Lex, Stage::Parse, Stage::Exec, Stage::Total] {
            if !stages.contains(&stage) {
                continue;
            }

            let json_path = tmp.join(format!("{}.{stage}.json", script.name));

            let dialect_flag = match script.dialect.as_str() {
                "bash" => " --bash",
                "bash44" => " --bash44",
                "bash50" => " --bash50",
                "bash51" => " --bash51",
                _ => "",
            };

            let thaum_cmd = match stage {
                Stage::Lex => format!("thaum --quiet{dialect_flag} lex {}", script.path.display()),
                Stage::Parse => format!("thaum --quiet{dialect_flag} parse {}", script.path.display()),
                Stage::Exec => {
                    // TODO: pre-parse to bincode payload, benchmark exec-ast --format binary
                    // For now, fall back to full exec (same as total).
                    format!("thaum{dialect_flag} exec {}", script.path.display())
                }
                Stage::Total => format!("thaum{dialect_flag} exec {}", script.path.display()),
            };

            eprintln!("  hyperfine: {} ({stage})", script.name);

            let mut cmd = Command::new("hyperfine");
            cmd.arg("--shell=none")
                .arg("--prepare=sync")
                .arg("--warmup=3")
                .arg("--min-runs=10")
                .arg("--ignore-failure")
                .arg(format!("--export-json={}", json_path.display()))
                .args(["--command-name", "thaum"])
                .arg(&thaum_cmd);

            // Only add reference shells for total (the standard comparison).
            if stage == Stage::Total {
                for sh in shells {
                    cmd.args(["--command-name", sh])
                        .arg(format!("{sh} {}", script.path.display()));
                }
            }

            if let Some(ref dir) = setup_dir {
                cmd.current_dir(dir);
            }

            let status = cmd
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .expect("failed to run hyperfine");

            if !status.success() {
                eprintln!("    hyperfine failed for {} ({stage})", script.name);
                continue;
            }

            let raw = std::fs::read_to_string(&json_path).unwrap_or_default();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(hf_results) = json["results"].as_array() {
                    // The first result is always thaum.
                    if let Some(r) = hf_results.first() {
                        let mean = r["mean"].as_f64().unwrap_or(0.0);
                        let stddev = r["stddev"].as_f64().unwrap_or(0.0);
                        let kind = Kind {
                            stage,
                            metric: Metric::Walltime,
                        };
                        measurements.insert(kind, Value::Time { mean, stddev });
                    }
                }
            }
        }

        if !measurements.is_empty() {
            // Merge into existing result for this script if callgrind already created one.
            if let Some(existing) = results.iter_mut().find(|r| r.name == script.name) {
                existing.measurements.extend(measurements);
            } else {
                results.push(BenchResult {
                    name: script.name.clone(),
                    measurements,
                });
            }
        }
    }

    let _ = std::fs::remove_dir_all(&tmp);
    results
}
