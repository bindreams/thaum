//! Unified test harness with runtime preconditions and unavailability reporting.
//!
//! Provides `#[testutil::test]` for annotating test functions. Tests can declare
//! runtime preconditions (e.g. "valgrind must be installed"), fixture injection,
//! custom display names, and labels for filtering. Tests whose preconditions are
//! not met show as `ignored` with an unavailability summary after all tests run.
//!
//! For dynamic test generation (e.g. from data files), use [`TestRunner::add`]
//! to register tests at runtime alongside attribute-registered ones.
//!
//! See the [README](../README.md) for usage instructions.

extern crate self as testutil;

pub mod docker;
pub mod fixture;
pub mod temp_dir;
pub mod test_name;

pub use fixture::{
    cleanup_process_fixtures, collect_fixture_requires, enter_test_scope, fixture, fixture_get, fixture_registry,
    warm_up, FixtureDef, FixtureHandle, FixtureRef, FixtureScope, TestScope,
};
pub use temp_dir::TempDir;
pub use test_name::TestName;

use std::cell::Cell;
use std::path::Path;
use std::process::{Command, Stdio};

use clap::Parser;
use libtest_mimic::{Arguments, Trial};

// Re-export proc macros for consumers.
pub use testutil_macros::fixture;
pub use testutil_macros::test;

// Re-export inventory so that macro-generated `inventory::submit!` calls resolve.
pub use inventory;

/// A precondition check function. Returns `Ok(())` if the requirement is met,
/// or `Err(reason)` if not.
pub type RequireFn = fn() -> Result<(), String>;

// Test context ================================================================

/// Metadata about the currently executing test, set by [`enter_test_scope`].
#[derive(Clone, Copy)]
pub struct CurrentTest {
    pub name: &'static str,
    pub module_path: &'static str,
}

thread_local! {
    pub(crate) static CURRENT_TEST: Cell<Option<CurrentTest>> = const { Cell::new(None) };
}

/// Get the current test context. Panics if called outside a test body.
pub fn current_test() -> CurrentTest {
    CURRENT_TEST.get().expect("called outside of a test body")
}

// Ignore ======================================================================

/// Whether a test is statically ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ignore {
    No,
    Yes,
    WithReason(&'static str),
}

// Test definition =============================================================

/// A test registered by `#[testutil::test(...)]` via inventory.
pub struct TestDef {
    pub name: &'static str,
    /// Module path (from `module_path!()`) for matching against `default_labels!`.
    pub module: &'static str,
    /// Display name (custom name). `None` → use `name`.
    pub display_name: Option<&'static str>,
    pub requires: &'static [RequireFn],
    /// Names of fixtures used by this test (from `#[fixture]` params).
    /// Used for transitive requirement collection via [`collect_fixture_requires`].
    pub fixture_names: &'static [&'static str],
    pub ignore: Ignore,
    /// Labels for filtering. Stored in libtest-mimic's `kind` field joined by `:`.
    pub labels: &'static [&'static str],
    /// Whether `labels = [...]` was explicitly written (even if empty).
    /// When false, module-level defaults from `default_labels!` apply.
    pub labels_explicit: bool,
    pub body: fn(),
}

inventory::collect!(TestDef);

// Module-level default labels =================================================

/// Default labels for all tests in a module. Registered by [`default_labels!`].
pub struct ModuleLabels {
    pub module: &'static str,
    pub labels: &'static [&'static str],
}

inventory::collect!(ModuleLabels);

/// Set default labels for all `#[testutil::test]` functions in the current module.
///
/// Tests that explicitly specify `labels = [...]` (including `labels = []`) are
/// not affected — explicit labels fully replace defaults.
///
/// ```ignore
/// testutil::default_labels!(docker, conformance);
///
/// #[testutil::test]                    // inherits [docker, conformance]
/// fn test_a() { ... }
///
/// #[testutil::test(labels = [slow])]   // gets [slow], not [docker, conformance, slow]
/// fn test_b() { ... }
///
/// #[testutil::test(labels = [])]       // gets nothing — explicit opt-out
/// fn test_c() { ... }
/// ```
#[macro_export]
macro_rules! default_labels {
    ($($label:ident),+ $(,)?) => {
        $crate::inventory::submit!($crate::ModuleLabels {
            module: ::core::module_path!(),
            labels: &[$(::core::stringify!($label)),+],
        });
    };
}

// Label filtering =============================================================

enum LabelSelector {
    Include(String),
    Exclude(String),
}

/// Extract `--label` arguments from the process args, returning the selectors
/// and the remaining args (for libtest-mimic).
///
/// Supports `--label docker`, `--label=docker,!slow`, and comma-separated values.
/// Use `!label` to exclude.
fn extract_label_filters() -> (Vec<LabelSelector>, Vec<String>) {
    let args: Vec<String> = std::env::args().collect();
    let mut selectors = Vec::new();
    let mut remaining = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--label" {
            i += 1;
            if i < args.len() {
                parse_label_arg(&args[i], &mut selectors);
            }
        } else if let Some(val) = args[i].strip_prefix("--label=") {
            parse_label_arg(val, &mut selectors);
        } else {
            remaining.push(args[i].clone());
        }
        i += 1;
    }

    (selectors, remaining)
}

fn parse_label_arg(val: &str, selectors: &mut Vec<LabelSelector>) {
    for part in val.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(label) = part.strip_prefix('!') {
            selectors.push(LabelSelector::Exclude(label.to_string()));
        } else {
            selectors.push(LabelSelector::Include(part.to_string()));
        }
    }
}

/// Check whether a test with the given labels passes the label filter.
///
/// - Includes form a union: test must match ANY include.
/// - Excludes subtract: test must not match ANY exclude.
/// - No includes → all tests included by default.
/// - No selectors → all tests pass.
fn label_matches(test_labels: &[&str], selectors: &[LabelSelector]) -> bool {
    let has_includes = selectors.iter().any(|s| matches!(s, LabelSelector::Include(_)));

    let included = if has_includes {
        selectors.iter().any(|s| match s {
            LabelSelector::Include(l) => test_labels.contains(&l.as_str()),
            LabelSelector::Exclude(_) => false,
        })
    } else {
        true
    };

    let excluded = selectors.iter().any(|s| match s {
        LabelSelector::Exclude(l) => test_labels.contains(&l.as_str()),
        LabelSelector::Include(_) => false,
    });

    included && !excluded
}

/// Resolve the effective labels for a test, applying module defaults if the test
/// did not explicitly specify `labels = [...]`.
fn resolve_labels(def: &TestDef, module_defaults: &[&ModuleLabels]) -> Vec<String> {
    if def.labels_explicit {
        return def.labels.iter().map(|s| s.to_string()).collect();
    }
    // Find the longest module prefix match.
    let default = module_defaults
        .iter()
        .filter(|m| def.module.starts_with(m.module))
        .max_by_key(|m| m.module.len());
    match default {
        Some(m) => m.labels.iter().map(|s| s.to_string()).collect(),
        None => def.labels.iter().map(|s| s.to_string()).collect(),
    }
}

// Test runner =================================================================

/// A dynamically-added test (registered at runtime, not via proc macro).
struct DynTest {
    name: String,
    ignored: bool,
    labels: Vec<String>,
    body: Box<dyn FnOnce() + Send + 'static>,
}

/// Collects tests from both `#[testutil::test]` (inventory) and runtime
/// [`add`](TestRunner::add) calls, then runs them via libtest-mimic.
#[derive(Default)]
pub struct TestRunner {
    dynamic: Vec<DynTest>,
    /// Custom args to strip before passing to libtest-mimic/clap.
    strip: Vec<String>,
}

impl TestRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register custom CLI args to strip before passing to libtest-mimic.
    ///
    /// Use for test-binary-specific flags (e.g. `--no-sandbox`) that would
    /// otherwise be rejected by the standard argument parser.
    pub fn strip_args(&mut self, args: &[&str]) -> &mut Self {
        self.strip.extend(args.iter().map(|s| s.to_string()));
        self
    }

    /// Add a test that was generated at runtime (e.g. from a data file).
    ///
    /// The `body` closure should panic on failure (like a normal test).
    pub fn add(
        &mut self,
        name: impl Into<String>,
        labels: &[&str],
        ignored: bool,
        body: impl FnOnce() + Send + 'static,
    ) {
        self.dynamic.push(DynTest {
            name: name.into(),
            ignored,
            labels: labels.iter().map(|s| s.to_string()).collect(),
            body: Box::new(body),
        });
    }

    /// Run all tests (inventory-registered + dynamic) and exit.
    pub fn run(self) -> ! {
        self.run_tests().exit();
    }

    /// Run all tests and return the conclusion for post-run assertions.
    pub fn run_tests(self) -> libtest_mimic::Conclusion {
        let (label_selectors, mut remaining_args) = extract_label_filters();
        remaining_args.retain(|a| !self.strip.contains(a));
        let args = Arguments::parse_from(remaining_args);
        let mut trials = Vec::new();
        let mut unavailable: Vec<(String, String)> = Vec::new();

        // Collect module-level default labels.
        let module_defaults: Vec<&ModuleLabels> = inventory::iter::<ModuleLabels>.into_iter().collect();

        // Inventory-registered tests (from #[testutil::test]).
        for def in inventory::iter::<TestDef> {
            let resolved = resolve_labels(def, &module_defaults);
            let resolved_refs: Vec<&str> = resolved.iter().map(|s| s.as_str()).collect();

            // Label filtering — skip entirely (not ignored, just absent).
            if !label_selectors.is_empty() && !label_matches(&resolved_refs, &label_selectors) {
                continue;
            }

            let trial_name = def.display_name.unwrap_or(def.name);
            let kind = resolved.join(":");

            // Static ignore — don't check preconditions, don't report as unavailable.
            if !matches!(def.ignore, Ignore::No) {
                let trial = Trial::test(trial_name, || Ok(())).with_ignored_flag(true);
                let trial = if kind.is_empty() { trial } else { trial.with_kind(kind) };
                trials.push(trial);
                continue;
            }

            // Collect requirements from both explicit requires and fixture dependencies.
            let fixture_requires = collect_fixture_requires(def.fixture_names);
            let reasons: Vec<String> = def
                .requires
                .iter()
                .chain(fixture_requires.iter())
                .filter_map(|check| check().err())
                .collect();

            if reasons.is_empty() {
                let body = def.body;
                let trial = Trial::test(trial_name, move || {
                    body();
                    Ok(())
                });
                let trial = if kind.is_empty() { trial } else { trial.with_kind(kind) };
                trials.push(trial);
            } else {
                let reason = reasons.join("; ");
                unavailable.push((trial_name.to_string(), reason));
                let trial = Trial::test(trial_name, || Ok(())).with_ignored_flag(true);
                let trial = if kind.is_empty() { trial } else { trial.with_kind(kind) };
                trials.push(trial);
            }
        }

        // Dynamic tests (from TestRunner::add).
        for dyn_test in self.dynamic {
            let dyn_labels: Vec<&str> = dyn_test.labels.iter().map(|s| s.as_str()).collect();
            if !label_selectors.is_empty() && !label_matches(&dyn_labels, &label_selectors) {
                continue;
            }

            let kind = dyn_test.labels.join(":");
            let body = dyn_test.body;
            // Leak the name for the 'static lifetime required by enter_test_scope.
            let name_static: &'static str = Box::leak(dyn_test.name.into_boxed_str());
            let trial = Trial::test(name_static, move || {
                // Auto-wrap dynamic tests in a test scope so fixtures are available.
                let _scope = enter_test_scope(name_static, "");
                body();
                Ok(())
            })
            .with_ignored_flag(dyn_test.ignored);
            let trial = if kind.is_empty() { trial } else { trial.with_kind(kind) };
            trials.push(trial);
        }

        let conclusion = libtest_mimic::run(&args, trials);

        // Clean up process-scoped fixtures (LIFO order).
        cleanup_process_fixtures();

        if !unavailable.is_empty() {
            eprintln!("\n--- Unavailable ({}) ---", unavailable.len());
            for (name, reason) in &unavailable {
                eprintln!("  {name}: {reason}");
            }
        }

        conclusion
    }
}

/// Shorthand: run only inventory-registered tests and exit.
pub fn run_all() -> ! {
    TestRunner::new().run();
}

// Precondition helpers ========================================================

/// Precondition: check that an executable is on PATH.
///
/// Returns `Ok(())` if `<name> --version` succeeds, or `Err` with a message.
pub fn probe_executable(name: &str) -> Result<(), String> {
    let ok = Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err(format!("{name} not installed"))
    }
}

/// Precondition: check that a file exists at the given path.
///
/// Returns `Ok(())` if the path exists, or `Err` with a message.
pub fn probe_path(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    if path.exists() {
        Ok(())
    } else {
        Err(format!("{} not found", path.display()))
    }
}
