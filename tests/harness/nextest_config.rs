//! Guards for the nextest filter expressions that CI and the timeout policy
//! depend on, and for the rule that enumerating tests has no side effects.
//!
//! Each guard reads its filter expression out of the file that actually
//! governs it — `.config/nextest.toml` or the workflow — and asks nextest to
//! evaluate it. Hardcoding the expressions here would let the guard drift away
//! from what CI runs, which is the failure these tests exist to catch.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::common::labels::{DOCKER, INFRA};

skuld::default_labels!(INFRA);

/// Binaries whose tests may build a Docker image.
///
/// `infra` builds both the gauntlet and bench images; `gauntlet` triggers a
/// build through the `gauntlet_sandbox` fixture. Both therefore need the long
/// slow-timeout, and neither belongs in a required CI job.
const DOCKER_BUILDING_BINARIES: &[&str] = &["thaum::infra", "thaum::gauntlet"];

fn nextest_available() -> Result<(), String> {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
        .then_some(())
        .ok_or_else(|| "cargo-nextest not installed".into())
}

fn docker_available() -> Result<(), String> {
    if thaum_testkit::docker::available() {
        Ok(())
    } else {
        Err("Docker not available".into())
    }
}

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path: PathBuf = project_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Strip ANSI SGR sequences.
///
/// Belt and braces: the cargo invocations below force colour off, but `ci.yml`
/// sets `CARGO_TERM_COLOR: always` workflow-wide, and a guard that parses
/// coloured output as if it were plain is exactly the failure mode that hid
/// itself once already.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for e in chars.by_ref() {
                if e.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Extract the value of a `key = "value"` line, given the key.
fn quoted_value_after(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.trim_start().strip_prefix('=')?;
    let rest = rest.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// The `filter` of the `[[profile.default.overrides]]` block granting the long
/// slow-timeout.
fn slow_timeout_override_filter() -> String {
    let toml = read(".config/nextest.toml");
    let mut in_override = false;
    for line in toml.lines() {
        if line.trim() == "[[profile.default.overrides]]" {
            in_override = true;
            continue;
        }
        if in_override {
            if let Some(v) = quoted_value_after(line, "filter") {
                return v;
            }
            if line.trim_start().starts_with('[') {
                in_override = false;
            }
        }
    }
    panic!("no `filter = \"...\"` found in a [[profile.default.overrides]] block of .config/nextest.toml");
}

/// The `-E '<expr>'` of the `cargo nextest run` step in the given workflow job.
fn workflow_filter(job: &str) -> String {
    let yaml = read(".github/workflows/ci.yml");
    let mut in_job = false;
    for line in yaml.lines() {
        // Job headers sit at exactly two spaces of indent under `jobs:`.
        if let Some(name) = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':')) {
            if !name.starts_with(' ') && !name.starts_with('-') {
                in_job = name == job;
                continue;
            }
        }
        if in_job && line.contains("cargo nextest run") {
            let after = line
                .split("-E ")
                .nth(1)
                .unwrap_or_else(|| panic!("job `{job}` runs nextest without an -E filter: {line}"));
            let after = after
                .trim_start()
                .strip_prefix('\'')
                .unwrap_or_else(|| panic!("job `{job}`: expected the -E expression in single quotes: {line}"));
            let end = after
                .find('\'')
                .unwrap_or_else(|| panic!("job `{job}`: unterminated -E expression: {line}"));
            return after[..end].to_string();
        }
    }
    panic!("no `cargo nextest run` step found in workflow job `{job}`");
}

/// Run a `cargo nextest` subcommand with colour forced off, returning stdout.
///
/// `CARGO_TERM_COLOR` is cleared explicitly: `--color never` governs only the
/// invocation it is passed to, while the environment variable is inherited by
/// everything underneath.
fn nextest_stdout(args: &[&str]) -> String {
    let output = Command::new("cargo")
        .arg("nextest")
        .args(args)
        .args(["--features", "cli", "--color", "never", "--cargo-quiet"])
        .env("CARGO_TERM_COLOR", "never")
        .current_dir(project_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|e| panic!("cargo nextest failed to start: {e}"));
    assert!(
        output.status.success(),
        "cargo nextest {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    strip_ansi(&String::from_utf8_lossy(&output.stdout))
}

/// Binary ids of every test nextest selects under `expr`.
fn binaries_selected_by(expr: &str) -> BTreeSet<String> {
    // Non-interactive listing prints one line per test: "<binary-id> <test name>".
    // Test names contain spaces, binary ids do not, so the id is the first field.
    nextest_stdout(&["list", "-E", expr])
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// Filesystem path of a built test binary, from nextest's own metadata.
fn test_binary_path(binary_id: &str) -> PathBuf {
    let json = nextest_stdout(&["list", "--list-type", "binaries-only", "--message-format", "json"]);
    let doc: serde_json::Value = serde_json::from_str(&json).expect("nextest binaries-only JSON");
    let path = doc["rust-binaries"][binary_id]["binary-path"]
        .as_str()
        .unwrap_or_else(|| panic!("no binary-path for {binary_id} in nextest metadata"));
    PathBuf::from(path)
}

// Filter guards -------------------------------------------------------------------------------------------------------

/// Every Docker-building binary that has listable tests is covered by the override.
///
/// Compared against the *unfiltered* listing rather than a fixed expectation:
/// skuld omits tests whose `requires` preconditions fail, so on a machine
/// without Docker every test in `thaum::infra` is unavailable and the binary
/// does not appear at all. Asserting it is present would then fail for a reason
/// that has nothing to do with the timeout policy — which is what happened on
/// the macOS CI runner.
#[skuld::test(requires = [nextest_available])]
fn slow_timeout_override_covers_every_docker_building_binary() {
    let filter = slow_timeout_override_filter();
    let selected = binaries_selected_by(&filter);
    let present = binaries_selected_by("all()");
    assert!(
        present.contains("thaum::gauntlet"),
        "no Docker-building binary has listable tests here, so this guard checked nothing.\n\
         Present: {present:?}"
    );
    for binary in DOCKER_BUILDING_BINARIES {
        if !present.contains(*binary) {
            continue;
        }
        assert!(
            selected.contains(*binary),
            "`{binary}` can build a Docker image but is not covered by the slow-timeout override \
             `{filter}`, so it gets the 30s default and is killed mid-build.\nSelected: {selected:?}"
        );
    }
}

#[skuld::test(requires = [nextest_available])]
fn gating_job_selects_no_docker_building_binary() {
    let filter = workflow_filter("test");
    let selected = binaries_selected_by(&filter);
    for binary in DOCKER_BUILDING_BINARIES {
        assert!(
            !selected.contains(*binary),
            "the gating CI job's filter `{filter}` selects `{binary}`, which builds Docker images \
             — a required check must not depend on the Docker build cache"
        );
    }
}

#[skuld::test(requires = [nextest_available])]
fn gating_job_runs_these_guards() {
    let filter = workflow_filter("test");
    let selected = binaries_selected_by(&filter);
    assert!(
        selected.contains("thaum::harness"),
        "the gating CI job's filter `{filter}` does not select `thaum::harness`, so these guards \
         would never run in CI.\nSelected: {selected:?}"
    );
}

/// The guards must survive the environment CI actually runs them in.
///
/// `ci.yml` sets `CARGO_TERM_COLOR: always` workflow-wide. An earlier version
/// of this file passed `--color never` to the outer command only, so the inner
/// `cargo nextest list` inherited the variable and emitted ANSI codes into the
/// output the assertions parse — and the guards failed in the one job they were
/// written to protect.
#[skuld::test(requires = [nextest_available])]
fn filter_parsing_survives_forced_colour() {
    let filter = workflow_filter("test");
    let output = Command::new("cargo")
        .args(["nextest", "list", "--features", "cli", "-E", &filter])
        .env("CARGO_TERM_COLOR", "always")
        .current_dir(project_root())
        .output()
        .expect("cargo nextest list");
    let parsed: BTreeSet<String> = strip_ansi(&String::from_utf8_lossy(&output.stdout))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_owned)
        .collect();
    assert!(
        parsed.contains("thaum::harness"),
        "binary ids could not be parsed out of coloured output — the guards would fail in CI, \
         where CARGO_TERM_COLOR=always.\nParsed: {parsed:?}"
    );
}

// Side-effect guard ---------------------------------------------------------------------------------------------------

/// Enumerating tests must build nothing (issue #20).
///
/// Asserts against the gauntlet binary's **own stderr**, not against
/// `cargo nextest list`'s output: nextest discards a test binary's stderr when
/// listing succeeds, so a guard phrased against the outer command's output can
/// never fail and reports success forever.
#[skuld::test(requires = [nextest_available, docker_available], labels = [DOCKER])]
fn listing_does_not_warm_up_the_docker_fixture() {
    let binary = test_binary_path("thaum::gauntlet");
    let output = Command::new(&binary)
        .arg("--list")
        .current_dir(project_root())
        .output()
        .unwrap_or_else(|e| panic!("running {} --list: {e}", binary.display()));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("building Docker image"),
        "`{} --list` warmed up the Docker fixture — enumerating tests must have no side effects \
         (issue #20).\nstderr:\n{stderr}",
        binary.display()
    );
}
