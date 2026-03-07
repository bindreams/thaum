//! Unified benchmark binary for thaum.
//!
//! Delegates to callgrind (instruction counts) and hyperfine (wall-clock)
//! backends based on the requested `--kind`.

mod bench {
    pub mod callgrind;
    pub mod docker;
    pub mod format;
    pub mod hyperfine_backend;
    pub mod types;
}

use bench::types::{BenchResult, Kind, Metric, Script};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

// CLI =================================================================================================================

#[derive(Parser)]
#[command(name = "bench", about = "Unified benchmarks for thaum")]
struct Cli {
    /// Measurement kinds as glob patterns (e.g. "lex.instructions", "*.walltime", "*").
    #[arg(long, default_value = "*")]
    kind: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Table)]
    format: Format,

    /// Path to a .sh.yaml file, .sh file, or directory of scripts.
    #[arg(long, default_value_os_t = default_scripts_path())]
    path: PathBuf,

    /// Run directly on host (skip Docker sandbox).
    #[arg(long)]
    no_sandbox: bool,

    /// Compare against a saved JSON baseline file.
    #[arg(long)]
    baseline_file: Option<PathBuf>,

    /// Path to the thaum binary (defaults to target/release/thaum).
    #[arg(long, default_value_os_t = default_thaum_exe())]
    thaum_exe: PathBuf,

    /// Ignored (injected by cargo bench).
    #[arg(long, hide = true)]
    bench: bool,
}

fn default_scripts_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/scripts")
}

fn default_thaum_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/thaum")
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum Format {
    Table,
    Json,
}

// Main ================================================================================================================

fn main() {
    let cli = Cli::parse();

    let kinds = bench::types::resolve_kinds(&cli.kind);
    if kinds.is_empty() {
        eprintln!("No kinds matched pattern: {}", cli.kind);
        std::process::exit(1);
    }
    let need_callgrind = kinds.iter().any(|k| k.metric.is_callgrind());
    let need_hyperfine = kinds.iter().any(|k| k.metric == Metric::Walltime);

    let scripts = bench::types::load_scripts(&cli.path);
    if scripts.is_empty() {
        eprintln!("No scripts found at {}", cli.path.display());
        std::process::exit(1);
    }

    if cli.no_sandbox {
        run_local(&cli, &kinds, &scripts, need_callgrind, need_hyperfine);
    } else {
        run_docker(&cli, &kinds);
    }
}

fn run_local(cli: &Cli, kinds: &[Kind], scripts: &[Script], need_callgrind: bool, need_hyperfine: bool) {
    let mut results: Vec<BenchResult> = Vec::new();

    if need_callgrind {
        let callgrind_kinds: Vec<Kind> = kinds.iter().copied().filter(|k| k.metric.is_callgrind()).collect();
        assert!(
            cli.thaum_exe.exists(),
            "thaum binary not found at {}\nRun: cargo build --release --features cli",
            cli.thaum_exe.display()
        );
        results.extend(bench::callgrind::run(&cli.thaum_exe, scripts, &callgrind_kinds));
    }

    if need_hyperfine {
        // Merge hyperfine results into existing script entries from callgrind.
        let walltime_results = bench::hyperfine_backend::run(scripts, kinds);
        for wr in walltime_results {
            if let Some(existing) = results.iter_mut().find(|r| r.name == wr.name) {
                existing.measurements.extend(wr.measurements);
            } else {
                results.push(wr);
            }
        }
    }

    output_results(&results, cli, kinds);
}

fn run_docker(cli: &Cli, kinds: &[Kind]) {
    if !bench::docker::available() {
        eprintln!("docker not available. Use --no-sandbox to run locally.");
        std::process::exit(1);
    }

    let Some(image_id) = bench::docker::build_image() else {
        eprintln!("Failed to build Docker image.");
        std::process::exit(1);
    };

    eprintln!("Running benchmarks inside Docker...\n");

    // Pass the original glob pattern to Docker — it will resolve there.
    let kinds_str = &cli.kind;

    // Determine the host path to bind-mount into the container.
    let host_path = if cli.path.is_dir() {
        cli.path.clone()
    } else {
        cli.path.parent().unwrap_or(&cli.path).to_path_buf()
    };
    let host_path = std::fs::canonicalize(&host_path).unwrap_or(host_path);

    // Inside the container, scripts are at /bench/scripts.
    let container_path = if cli.path.is_dir() {
        "/bench/scripts".to_string()
    } else {
        let fname = cli.path.file_name().unwrap().to_string_lossy();
        format!("/bench/scripts/{fname}")
    };

    let mount = format!("{}:/bench/scripts:ro", host_path.display());
    let stdout = bench::docker::run_with_volume(
        &image_id,
        &mount,
        &[
            "--no-sandbox",
            "--format",
            "json",
            "--kind",
            kinds_str,
            "--path",
            &container_path,
            "--thaum-exe",
            "/usr/local/bin/thaum",
        ],
    );

    let Some(stdout) = stdout else {
        eprintln!("Docker run failed.");
        std::process::exit(1);
    };

    let json_str = String::from_utf8_lossy(&stdout);

    if cli.format == Format::Json {
        print!("{json_str}");
        return;
    }

    let results: Vec<BenchResult> = json_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    output_results(&results, cli, kinds);
}

fn output_results(results: &[BenchResult], cli: &Cli, kinds: &[Kind]) {
    if results.is_empty() {
        eprintln!("No benchmark results.");
        std::process::exit(1);
    }

    if cli.format == Format::Json {
        for r in results {
            if let Ok(line) = serde_json::to_string(r) {
                println!("{line}");
            }
        }
        return;
    }

    let baseline = cli.baseline_file.as_ref().map(|path| {
        let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("cannot read baseline file: {e}");
            std::process::exit(1);
        });
        bench::format::parse_baseline(&content)
    });

    let table = bench::format::build_table(results, kinds, baseline.as_ref());
    print!("{table}");
}
