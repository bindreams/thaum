//! Resolving a command name to an executable using the shell's own `$PATH`.
//!
//! Lookup is shell state, not process state: it reads the `PATH` variable of
//! the running shell (exported or not), resolves relative and empty entries
//! against the shell's cwd, and produces the 126/127 statuses the shell
//! reports. The spawn layer receives an already-resolved path and searches
//! nothing.
//!
//! An absent or empty `PATH` is a single empty entry, which means the shell's
//! cwd (POSIX XBD 8.3) — so `PATH=; cmd` still runs `cmd` from the cwd, but
//! reaches nothing else.
//!
//! That "nothing else" is not an assumption; it is forced by a pair of measured
//! results. In bash 5.3 under `env -i`, `unset PATH; ls` fails with 127 while
//! `unset PATH; ./tool-in-cwd` runs. If an unset `PATH` fell back to the host
//! environment or to `confstr(_CS_PATH)`, `ls` would have been found; if it
//! searched nothing at all, the cwd tool would not have been. Only "the cwd,
//! and solely the cwd" satisfies both. Bash reserves the `confstr` default for
//! `command -p`, which thaum does not implement.
//!
//! Beware of confirming this with a probe whose cwd lacks the tool: an
//! empty-`PATH`-means-cwd shell and an empty-`PATH`-fails shell both answer 127
//! there, so such a probe distinguishes nothing.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::exec::io_context::IoContext;
use crate::exec::Executor;

/// Why a command name could not be turned into something spawnable.
#[derive(Debug)]
pub(crate) enum LookupError {
    /// No candidate matched, or `PATH` was absent or empty.
    NotFound,
    /// A regular file matched but is not executable. Carries the first such
    /// candidate — the one bash names in its diagnostic.
    ///
    /// Never produced on Windows, which has no execute bit: a file that does
    /// not match `PATHEXT` is simply not a candidate.
    #[cfg_attr(windows, allow(dead_code))]
    NotExecutable(PathBuf),
}

/// Separator between `PATH` entries.
const PATH_SEPARATOR: char = if cfg!(windows) { ';' } else { ':' };

/// Whether `name` denotes a path rather than a bare command name.
fn has_path_separator(name: &str) -> bool {
    name.contains('/') || (cfg!(windows) && name.contains('\\'))
}

/// Whether `path` is a regular file that may be executed.
///
/// Directories are excluded even though they carry the execute bit. On Windows
/// there is no execute bit; executability is decided by extension, which
/// `resolve_windows` handles when it generates candidates.
#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Whether `path` is a regular file, executable or not.
#[cfg(not(windows))]
fn is_regular_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

/// Turn one `PATH` entry into a directory, resolving it against the shell's
/// cwd. An empty entry means the cwd, as does any relative entry.
///
/// The empty case is load-bearing, not decorative: an empty or absent `PATH`
/// splits into exactly one empty entry, so this branch is what makes
/// `PATH=; cmd` and `unset PATH; cmd` search the cwd the way bash does. It is
/// spelled out rather than left to the relative branch, where correctness would
/// rest on `Path::join("")` incidentally yielding an unchanged path.
fn entry_to_dir(entry: &str, cwd: &Path) -> PathBuf {
    if entry.is_empty() {
        return cwd.to_path_buf();
    }
    let p = Path::new(entry);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

/// Resolve `name` to an executable path.
///
/// A `name` containing a path separator is returned unchanged: bash performs no
/// `PATH` search for it, and the caller spawns it relative to the shell's cwd.
/// `pathext` is the shell's `PATHEXT` and is consulted on Windows only.
#[cfg_attr(debug_assertions, contracts::debug_ensures(
    has_path_separator(name) || ret.as_ref().is_ok_and(|p| p.is_absolute()) || ret.is_err(),
    "a searched name must resolve to an absolute path"
))]
#[cfg_attr(not(windows), allow(unused_variables))]
pub(crate) fn resolve(
    name: &str,
    path_var: Option<&str>,
    cwd: &Path,
    pathext: Option<&str>,
) -> Result<PathBuf, LookupError> {
    if has_path_separator(name) {
        return Ok(PathBuf::from(name));
    }

    // An absent `PATH` is treated as an empty one, which splits into a single
    // empty entry — i.e. the shell's cwd, and nothing else. There is no fallback
    // to the host environment or to `confstr(_CS_PATH)`.
    let path_var = path_var.unwrap_or("");

    let dirs: Vec<PathBuf> = path_var.split(PATH_SEPARATOR).map(|e| entry_to_dir(e, cwd)).collect();

    #[cfg(windows)]
    {
        // Windows candidate generation (PATHEXT modes) lives in
        // `resolve_windows`; feed it directories already resolved against the
        // shell's cwd.
        let absolute = dirs
            .iter()
            .map(|d| d.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(";");
        return match crate::exec::command_ex::resolve_windows::resolve_command(OsStr::new(name), &absolute, pathext) {
            Some(p) => Ok(p),
            None => Err(LookupError::NotFound),
        };
    }

    #[cfg(not(windows))]
    {
        let mut first_non_executable = None;
        for dir in &dirs {
            let candidate = dir.join(name);
            if is_executable_file(&candidate) {
                return Ok(candidate);
            }
            if first_non_executable.is_none() && is_regular_file(&candidate) {
                first_non_executable = Some(candidate);
            }
        }
        match first_non_executable {
            Some(p) => Err(LookupError::NotExecutable(p)),
            None => Err(LookupError::NotFound),
        }
    }
}

/// Write the shell's diagnostic for `err` and return the exit status it implies.
///
/// A missing command is 127 and names what the user typed; an unusable match is
/// 126 and names the file that was rejected, since that identifies which `PATH`
/// entry produced it.
pub(crate) fn report(err: &LookupError, name: &str, io: &mut IoContext) -> i32 {
    match err {
        LookupError::NotFound => {
            if let Some(stderr) = io.fd_mut(2) {
                let _ = writeln!(stderr, "{name}: command not found");
            }
            127
        }
        LookupError::NotExecutable(path) => {
            if let Some(stderr) = io.fd_mut(2) {
                let _ = writeln!(stderr, "{}: Permission denied", path.display());
            }
            126
        }
    }
}

impl Executor {
    /// Resolve `name` for a child process whose environment is `child_env`.
    ///
    /// `child_env` holds the exported variables plus this command's prefix
    /// assignments, so `PATH=dir cmd` searches `dir` for `cmd` itself. When it
    /// carries no `PATH`, the shell variable is used — bash honours `PATH` for
    /// lookup whether or not it was exported.
    pub(crate) fn lookup_command(
        &self,
        name: &str,
        child_env: &HashMap<OsString, OsString>,
    ) -> Result<PathBuf, LookupError> {
        let from_child = |key: &str| child_env.get(OsStr::new(key)).and_then(|v| v.to_str());
        let path_var = from_child("PATH").or_else(|| self.env.get_var("PATH"));
        let pathext = from_child("PATHEXT").or_else(|| self.env.get_var("PATHEXT"));
        resolve(name, path_var, self.env.cwd(), pathext)
    }
}

#[cfg(test)]
#[path = "command_lookup_tests.rs"]
mod tests;
