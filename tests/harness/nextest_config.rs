//! Guards for the nextest filter expressions that CI and the timeout policy
//! depend on.
//!
//! Both tests read their filter expression out of the file that actually
//! governs it — `.config/nextest.toml` or the workflow — and ask nextest to
//! evaluate it. Hardcoding the expressions here would let the guard drift away
//! from what CI runs, which is the failure these tests exist to catch.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::common::labels::INFRA;

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

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path: PathBuf = project_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Extract the value of a `key = "value"` line, given the key.
fn quoted_value_after(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.trim_start().strip_prefix('=')?;
    let rest = rest.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// The `filter` of the `[[profile.default.overrides]]` block that grants the
/// long slow-timeout.
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

/// Run `cargo nextest list -E expr`, returning (stdout, stderr).
fn nextest_list(expr: &str) -> (String, String) {
    let output = Command::new("cargo")
        .args(["nextest", "list", "--features", "cli", "--cargo-quiet", "-E", expr])
        .current_dir(project_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|e| panic!("cargo nextest list failed to start: {e}"));
    assert!(
        output.status.success(),
        "cargo nextest list -E {expr:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Binary ids of every test nextest selects under `expr`.
fn binaries_selected_by(expr: &str) -> BTreeSet<String> {
    let (stdout, _) = nextest_list(expr);
    // Non-interactive listing prints one line per test: "<binary-id> <test name>".
    // Test names contain spaces, binary ids do not, so the id is the first field.
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// Listing tests must not build anything.
///
/// `cargo nextest list` runs every selected binary with `--list`, and the
/// gauntlet binary used to warm up its Docker fixture from `main()` — so
/// enumerating tests built an image (issue #20). The guards above evaluate a
/// filter that selects the gauntlet binary, so a regression here would make
/// them slow and Docker-dependent rather than cheap.
#[skuld::test(requires = [nextest_available])]
fn listing_tests_does_not_build_docker_images() {
    let filter = slow_timeout_override_filter();
    let (_, stderr) = nextest_list(&filter);
    assert!(
        !stderr.contains("building Docker image"),
        "listing tests under `{filter}` built a Docker image — enumeration must have no side \
         effects (issue #20).\nstderr:\n{stderr}"
    );
}

#[skuld::test(requires = [nextest_available])]
fn slow_timeout_override_covers_every_docker_building_binary() {
    let filter = slow_timeout_override_filter();
    let selected = binaries_selected_by(&filter);
    for binary in DOCKER_BUILDING_BINARIES {
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
