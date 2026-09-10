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

/// Binaries containing tests that may build a Docker image.
///
/// Four of `infra`'s six tests build the gauntlet or bench image directly; the
/// other two are callgrind benchmarks that build nothing. `gauntlet` triggers a
/// build through the `gauntlet_sandbox` fixture. The timeout override and the
/// CI exclusion are both whole-binary, so this list is of binaries, not tests.
const DOCKER_BUILDING_BINARIES: &[&str] = &["thaum::infra", "thaum::gauntlet"];

/// Binaries needing the long slow-timeout.
///
/// A superset of the above: `harness` builds no image in normal operation, but
/// `listing_tests_builds_no_image` triggers one precisely when the regression it
/// guards is present. Under the 30s default that surfaces as "test timed out"
/// instead of the guard's diagnostic, and leaves a `docker build` running
/// daemon-side.
const SLOW_TIMEOUT_BINARIES: &[&str] = &["thaum::infra", "thaum::gauntlet", "thaum::harness"];

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
/// `ci.yml` sets `CARGO_TERM_COLOR: always` workflow-wide, so colour is forced
/// off on both the outer and inner invocations; this is the remaining defence if
/// either is missed.
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

/// The `filter` of the override block that sets `slow-timeout`.
///
/// nextest allows arbitrarily many `[[profile.default.overrides]]` blocks, so
/// taking the first one silently reads whichever override happens to come first
/// — a later `retries` or `test-group` block would redirect this guard onto an
/// unrelated filter. Selects by the key that matters and fails if the answer is
/// not unique.
fn slow_timeout_override_filter() -> String {
    let toml = read(".config/nextest.toml");
    let mut matching: Vec<String> = Vec::new();
    for block in toml.split("[[profile.default.overrides]]").skip(1) {
        // A block ends at the next section header.
        let block: String = block
            .lines()
            .take_while(|l| !l.trim_start().starts_with('['))
            .collect::<Vec<_>>()
            .join("\n");
        let filter = block.lines().find_map(|l| quoted_value_after(l, "filter"));
        let sets_timeout = block.lines().any(|l| l.trim_start().starts_with("slow-timeout"));
        if let (Some(f), true) = (filter, sets_timeout) {
            matching.push(f);
        }
    }
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one [[profile.default.overrides]] block setting slow-timeout in \
         .config/nextest.toml, found {}: {matching:?}",
        matching.len()
    );
    matching.remove(0)
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

/// The Docker-building binaries, confirmed enumerable before anything is
/// asserted about them.
///
/// Every guard below asserts something *about* these binaries, and an assertion
/// over an absent binary passes having checked nothing: a loop that runs zero
/// times reports PASS. That is how two earlier versions of these guards
/// reported green while broken.
///
/// skuld omits tests whose `requires` preconditions fail, so where Docker is
/// missing every test in `thaum::infra` is unavailable and the binary does not
/// appear at all. Preconditions therefore belong in `requires`, where an unmet
/// one is reported as unavailable, never as a silent skip inside a test body.
fn listable(binaries: &[&'static str]) -> Vec<&'static str> {
    let present = binaries_selected_by("all()");
    let listable: Vec<&'static str> = binaries.iter().copied().filter(|b| present.contains(*b)).collect();
    assert_eq!(
        listable.len(),
        binaries.len(),
        "expected every Docker-building binary to be enumerable, but only {listable:?} are. \
         Any assertion about the missing ones would pass having checked nothing.\nPresent: {present:?}"
    );
    listable
}

/// Every Docker-building binary is covered by the slow-timeout override.
#[skuld::test(requires = [nextest_available, docker_available])]
fn slow_timeout_override_covers_every_docker_building_binary() {
    let filter = slow_timeout_override_filter();
    let selected = binaries_selected_by(&filter);
    for binary in listable(SLOW_TIMEOUT_BINARIES) {
        assert!(
            selected.contains(binary),
            "`{binary}` can build a Docker image but is not covered by the slow-timeout override \
             `{filter}`, so it gets the 30s default and is killed mid-build.\nSelected: {selected:?}"
        );
    }
}

#[skuld::test(requires = [nextest_available, docker_available])]
fn gating_job_selects_no_docker_building_binary() {
    let filter = workflow_filter("test");
    let selected = binaries_selected_by(&filter);
    // An assertion of absence is vacuous twice over: against an empty selection,
    // and against a binary that cannot be enumerated here at all.
    assert!(
        !selected.is_empty(),
        "the gating CI job's filter `{filter}` selected no tests at all, so asserting what it \
         does not select proves nothing"
    );
    for binary in listable(DOCKER_BUILDING_BINARIES) {
        assert!(
            !selected.contains(binary),
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

/// Binary ids parse out of coloured output.
///
/// `ci.yml` sets `CARGO_TERM_COLOR: always` workflow-wide, and it is inherited by
/// the inner `cargo nextest list` unless cleared — `--color never` on the outer
/// command alone does not reach it. This guard forces colour off on both and
/// checks parsing under a forced-colour environment.
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

/// Enumerating tests must build nothing.
///
/// Asserts against the gauntlet binary's **own stderr**: nextest discards a test
/// binary's stderr when listing succeeds, so a guard phrased against the outer
/// command's output can never fail.
///
/// The child's environment is scrubbed of the two variables that would make this
/// pass without checking anything. `THAUM_GAUNTLET_NO_SANDBOX` short-circuits the
/// code path under test; `SKULD_LABELS` filters the child's listing, so an
/// inherited value can empty it. Both are documented developer knobs, so both are
/// plausible in the environment a developer runs this from.
///
/// Skipped where Docker is absent, which is correct — without a daemon no warm-up
/// is attempted — but it does mean this guard does not run on macOS CI.
#[skuld::test(requires = [nextest_available, docker_available], labels = [DOCKER])]
fn listing_tests_builds_no_image() {
    let binary = test_binary_path("thaum::gauntlet");
    let output = Command::new(&binary)
        .arg("--list")
        .env_remove("THAUM_GAUNTLET_NO_SANDBOX")
        .env_remove("SKULD_LABELS")
        .current_dir(project_root())
        .output()
        .unwrap_or_else(|e| panic!("running {} --list: {e}", binary.display()));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let listed = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "`{} --list` exited {:?}; a child that dies early emits no marker either.\nstderr:\n{stderr}",
        binary.display(),
        output.status.code()
    );
    assert!(
        listed.lines().any(|l| !l.trim().is_empty()),
        "`{} --list` listed nothing, so the absence of a build marker proves nothing",
        binary.display()
    );
    assert!(
        !stderr.contains(crate::common::docker::IMAGE_BUILD_MARKER),
        "`{} --list` warmed up the Docker fixture; enumerating tests must have no side \
         effects.\nstderr:\n{stderr}",
        binary.display()
    );
}

/// `CONTRIBUTING.md` documents the Docker-free filter by copying it.
///
/// Nothing otherwise keeps that copy in step with `ci.yml`, and a developer
/// following stale instructions gets Docker builds they were told they had
/// opted out of.
#[skuld::test]
fn contributing_documents_the_gating_filter_verbatim() {
    let filter = workflow_filter("test");
    let doc = read("CONTRIBUTING.md");
    assert!(
        doc.contains(&filter),
        "CONTRIBUTING.md does not contain the gating job's filter `{filter}` verbatim, so the \
         documented way to skip Docker work has drifted from what CI runs"
    );
}
