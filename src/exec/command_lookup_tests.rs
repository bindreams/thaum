use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use skuld::temp_dir;

use super::{report, resolve, LookupError};
use crate::exec::io_context::CapturedIo;
use crate::exec::{Environment, Executor};
use crate::test_labels::EXEC;

skuld::default_labels!(EXEC);

/// Create an executable file at `path`. On Unix the execute bits are set; on
/// Windows executability is decided by extension, so mode is irrelevant.
fn write_executable(path: &Path) {
    std::fs::write(path, b"").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Create a regular file that is not executable.
#[cfg(unix)]
fn write_non_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, b"").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
}

/// Join directories into a `PATH` string using the platform separator.
fn path_var(dirs: &[&Path]) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    dirs.iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(sep)
}

/// Name of the command under test, with the extension Windows needs to
/// consider a file executable.
fn cmd_name() -> &'static str {
    if cfg!(windows) {
        "mytool.exe"
    } else {
        "mytool"
    }
}

fn mkdir(parent: &Path, name: &str) -> PathBuf {
    let d = parent.join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// Names that bypass the search ========================================================================================

#[skuld::test]
fn name_with_separator_is_returned_unsearched(#[fixture(temp_dir)] dir: &Path) {
    let a = mkdir(dir, "a");
    write_executable(&a.join(cmd_name()));

    // The name has a separator, so `PATH` is irrelevant — even a `PATH` that
    // contains a match must not redirect it.
    let result = resolve("./sub/mytool", Some(&path_var(&[&a])), dir, None).unwrap();
    assert_eq!(result, PathBuf::from("./sub/mytool"));
}

// Search order ========================================================================================================

#[skuld::test]
fn first_executable_match_wins(#[fixture(temp_dir)] dir: &Path) {
    let a = mkdir(dir, "a");
    let b = mkdir(dir, "b");
    write_executable(&a.join(cmd_name()));
    write_executable(&b.join(cmd_name()));

    let result = resolve(cmd_name(), Some(&path_var(&[&a, &b])), dir, None).unwrap();
    assert_eq!(result, a.join(cmd_name()));
}

#[skuld::test]
fn no_match_anywhere_is_not_found(#[fixture(temp_dir)] dir: &Path) {
    let a = mkdir(dir, "a");
    let err = resolve(cmd_name(), Some(&path_var(&[&a])), dir, None).unwrap_err();
    assert!(matches!(err, LookupError::NotFound), "expected NotFound");
}

#[skuld::test]
fn directory_match_is_skipped(#[fixture(temp_dir)] dir: &Path) {
    let a = mkdir(dir, "a");
    let b = mkdir(dir, "b");
    // A *directory* named like the command must never match, even though
    // directories carry the execute bit.
    mkdir(&a, cmd_name());
    write_executable(&b.join(cmd_name()));

    let result = resolve(cmd_name(), Some(&path_var(&[&a, &b])), dir, None).unwrap();
    assert_eq!(result, b.join(cmd_name()));

    let err = resolve(cmd_name(), Some(&path_var(&[&a])), dir, None).unwrap_err();
    assert!(matches!(err, LookupError::NotFound), "expected NotFound");
}

// Non-executable matches ==============================================================================================

#[cfg(unix)]
#[skuld::test]
fn search_continues_past_non_executable_match(#[fixture(temp_dir)] dir: &Path) {
    let a = mkdir(dir, "a");
    let b = mkdir(dir, "b");
    write_non_executable(&a.join(cmd_name()));
    write_executable(&b.join(cmd_name()));

    let result = resolve(cmd_name(), Some(&path_var(&[&a, &b])), dir, None).unwrap();
    assert_eq!(result, b.join(cmd_name()));
}

#[cfg(unix)]
#[skuld::test]
fn only_non_executable_match_is_not_executable_error(#[fixture(temp_dir)] dir: &Path) {
    let a = mkdir(dir, "a");
    write_non_executable(&a.join(cmd_name()));

    match resolve(cmd_name(), Some(&path_var(&[&a])), dir, None) {
        Err(LookupError::NotExecutable(p)) => assert_eq!(p, a.join(cmd_name())),
        other => panic!("expected NotExecutable, got {:?}", other.is_ok()),
    }
}

#[cfg(unix)]
#[skuld::test]
fn first_non_executable_is_the_one_reported(#[fixture(temp_dir)] dir: &Path) {
    let a = mkdir(dir, "a");
    let b = mkdir(dir, "b");
    write_non_executable(&a.join(cmd_name()));
    write_non_executable(&b.join(cmd_name()));

    match resolve(cmd_name(), Some(&path_var(&[&a, &b])), dir, None) {
        Err(LookupError::NotExecutable(p)) => assert_eq!(p, a.join(cmd_name())),
        other => panic!("expected NotExecutable, got {:?}", other.is_ok()),
    }
}

// Absent and empty PATH ===============================================================================================

// An empty `PATH` is a single empty entry, and an empty entry means the current
// directory (POSIX XBD 8.3). An unset `PATH` behaves identically. Neither falls
// back to the host environment or to `confstr(_CS_PATH)` — measured in bash 5.3
// under `env -i`, where `unset PATH; ls` is 127 but `unset PATH; ./tool-in-cwd`
// runs.

#[skuld::test]
fn absent_path_var_searches_cwd(#[fixture(temp_dir)] dir: &Path) {
    let cwd = mkdir(dir, "cwd");
    write_executable(&cwd.join(cmd_name()));

    let result = resolve(cmd_name(), None, &cwd, None).unwrap();
    assert_eq!(result, cwd.join(cmd_name()));
}

#[skuld::test]
fn empty_path_var_searches_cwd(#[fixture(temp_dir)] dir: &Path) {
    let cwd = mkdir(dir, "cwd");
    write_executable(&cwd.join(cmd_name()));

    let result = resolve(cmd_name(), Some(""), &cwd, None).unwrap();
    assert_eq!(result, cwd.join(cmd_name()));
}

#[skuld::test]
fn absent_path_var_does_not_reach_beyond_cwd(#[fixture(temp_dir)] dir: &Path) {
    let cwd = mkdir(dir, "cwd");
    let elsewhere = mkdir(dir, "elsewhere");
    write_executable(&elsewhere.join(cmd_name()));

    // Only the cwd is searched — no host `PATH`, no `confstr` default.
    let err = resolve(cmd_name(), None, &cwd, None).unwrap_err();
    assert!(matches!(err, LookupError::NotFound), "expected NotFound");
    let err = resolve(cmd_name(), Some(""), &cwd, None).unwrap_err();
    assert!(matches!(err, LookupError::NotFound), "expected NotFound");
}

// PATH entries resolved against the shell's cwd =======================================================================

#[skuld::test]
fn empty_path_entry_means_cwd(#[fixture(temp_dir)] dir: &Path) {
    let cwd = mkdir(dir, "cwd");
    let other = mkdir(dir, "other");
    write_executable(&cwd.join(cmd_name()));

    let sep = if cfg!(windows) { ";" } else { ":" };
    let leading = format!("{sep}{}", other.to_string_lossy());
    let trailing = format!("{}{sep}", other.to_string_lossy());
    let doubled = format!("{}{sep}{sep}{}", other.to_string_lossy(), other.to_string_lossy());

    for var in [leading, trailing, doubled] {
        let result = resolve(cmd_name(), Some(&var), &cwd, None)
            .unwrap_or_else(|_| panic!("empty entry in {var:?} should resolve to cwd"));
        assert_eq!(result, cwd.join(cmd_name()));
    }
}

#[skuld::test]
fn relative_path_entry_resolves_against_cwd(#[fixture(temp_dir)] dir: &Path) {
    let cwd = mkdir(dir, "cwd");
    let tools = mkdir(dir, "tools");
    write_executable(&tools.join(cmd_name()));

    // `../tools` is relative to the *shell's* cwd, not the process cwd.
    let result = resolve(cmd_name(), Some("../tools"), &cwd, None).unwrap();
    assert_eq!(result, cwd.join("../tools").join(cmd_name()));
}

#[skuld::test]
fn dot_path_entry_means_cwd(#[fixture(temp_dir)] dir: &Path) {
    let cwd = mkdir(dir, "cwd");
    write_executable(&cwd.join(cmd_name()));

    let result = resolve(cmd_name(), Some("."), &cwd, None).unwrap();
    assert_eq!(result, cwd.join(".").join(cmd_name()));
}

// Which PATH the shell uses ===========================================================================================

/// An executor whose cwd is `cwd` and whose shell `PATH` is `path`.
fn executor_with(path: Option<&str>, cwd: &Path) -> Executor {
    let mut env = Environment::new();
    env.set_cwd(cwd.to_path_buf()).unwrap();
    if let Some(p) = path {
        env.set_var("PATH", p).unwrap();
    }
    Executor::with_env(env)
}

fn child_env(pairs: &[(&str, &str)]) -> HashMap<OsString, OsString> {
    pairs
        .iter()
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect()
}

#[skuld::test]
fn unexported_shell_path_is_used_when_child_env_has_none(#[fixture(temp_dir)] dir: &Path) {
    let a = mkdir(dir, "a");
    write_executable(&a.join(cmd_name()));

    // `set_var` does not export, so `PATH` never reaches the child environment.
    // Lookup must use it anyway.
    let executor = executor_with(Some(&path_var(&[&a])), dir);
    let result = executor.lookup_command(cmd_name(), &child_env(&[])).unwrap();
    assert_eq!(result, a.join(cmd_name()));
}

#[skuld::test]
fn prefix_assignment_path_overrides_shell_variable(#[fixture(temp_dir)] dir: &Path) {
    let shell_dir = mkdir(dir, "shell");
    let prefix_dir = mkdir(dir, "prefix");
    write_executable(&shell_dir.join(cmd_name()));
    write_executable(&prefix_dir.join(cmd_name()));

    // `PATH=<prefix_dir> mytool` must find `mytool` in `prefix_dir`.
    let executor = executor_with(Some(&path_var(&[&shell_dir])), dir);
    let env = child_env(&[("PATH", &path_var(&[&prefix_dir]))]);
    let result = executor.lookup_command(cmd_name(), &env).unwrap();
    assert_eq!(result, prefix_dir.join(cmd_name()));
}

#[skuld::test]
fn shell_without_path_variable_searches_cwd(#[fixture(temp_dir)] dir: &Path) {
    let cwd = mkdir(dir, "cwd");
    write_executable(&cwd.join(cmd_name()));
    let executor = executor_with(None, &cwd);
    let result = executor.lookup_command(cmd_name(), &child_env(&[])).unwrap();
    assert_eq!(result, cwd.join(cmd_name()));
}

// Diagnostics =========================================================================================================

/// Run `report` against a captured `IoContext` and return `(status, stderr)`.
fn captured_report(err: &LookupError, name: &str) -> (i32, String) {
    let (mut io, capture) = CapturedIo::new();
    let status = report(err, name, &mut io);
    let output = capture.finish(io);
    (status, output.stderr_string())
}

#[skuld::test]
fn report_not_found_writes_command_not_found_and_returns_127() {
    let (status, stderr) = captured_report(&LookupError::NotFound, "mytool");
    assert_eq!(status, 127);
    assert_eq!(stderr, "mytool: command not found\n");
}

#[skuld::test]
fn report_not_executable_names_the_resolved_path_and_returns_126() {
    // bash names the rejected file, not the typed name: the path is what
    // identifies which `PATH` entry produced the unusable match.
    let err = LookupError::NotExecutable(PathBuf::from("/tools/mytool"));
    let (status, stderr) = captured_report(&err, "mytool");
    assert_eq!(status, 126);
    assert_eq!(stderr, "/tools/mytool: Permission denied\n");
}
