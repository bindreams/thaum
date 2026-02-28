//! Runtime test preconditions with unavailability reporting.
//!
//! Provides `#[requires(...)]` for annotating test functions with runtime
//! preconditions (e.g. "valgrind must be installed"). Tests whose preconditions
//! are not met show as `ignored` in the test output, and an unavailability
//! summary is printed after all tests complete.
//!
//! See the [README](../README.md) for usage instructions.

pub mod docker;

use std::path::Path;
use std::process::{Command, Stdio};

use libtest_mimic::{Arguments, Trial};

// Re-export the proc macro for consumers.
pub use testutil_macros::requires;

// Re-export inventory so that macro-generated `inventory::submit!` calls resolve.
pub use inventory;

/// A test definition registered by `#[requires(...)]`.
pub struct TestDef {
    pub name: &'static str,
    pub requires: &'static [fn() -> Result<(), String>],
    pub body: fn(),
}

inventory::collect!(TestDef);

/// Collect all `#[requires]`-registered tests, check preconditions, and run
/// them via libtest-mimic. Prints an unavailability summary for skipped tests.
///
/// Calls `process::exit()` with the appropriate exit code.
pub fn run_all() -> ! {
    run_tests().exit();
}

/// Like [`run_all`], but returns the conclusion instead of exiting. Useful when
/// you need to run post-test assertions before exiting.
pub fn run_tests() -> libtest_mimic::Conclusion {
    let args = Arguments::from_args();
    let mut trials = Vec::new();
    let mut unavailable: Vec<(&str, String)> = Vec::new();

    for def in inventory::iter::<TestDef> {
        let reasons: Vec<String> = def.requires.iter().filter_map(|check| check().err()).collect();

        if reasons.is_empty() {
            let body = def.body;
            trials.push(Trial::test(def.name, move || {
                body();
                Ok(())
            }));
        } else {
            let reason = reasons.join("; ");
            unavailable.push((def.name, reason));
            trials.push(Trial::test(def.name, || Ok(())).with_ignored_flag(true));
        }
    }

    let conclusion = libtest_mimic::run(&args, trials);

    if !unavailable.is_empty() {
        eprintln!("\n--- Unavailable ({}) ---", unavailable.len());
        for (name, reason) in &unavailable {
            eprintln!("  {name}: {reason}");
        }
    }

    conclusion
}

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
