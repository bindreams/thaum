//! Windows command path resolution: search PATH directories with PATHEXT support.
//!
//! Two modes based on whether `PATHEXT` is set:
//! - **Unset** (MSYS2 rules): try exact name, then `.exe`
//! - **Set** (cmd.exe rules): try only PATHEXT-appended names

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Resolve a bare command name to a full executable path by searching PATH.
///
/// `pathext` selects the resolution mode:
/// - `None` → MSYS2 rules: exact name first, then `.exe`
/// - `Some(exts)` → cmd.exe rules: only PATHEXT-appended names (semicolon-separated)
///
/// Returns `None` if the name contains path separators (`/` or `\`) or no
/// match is found.
pub(crate) fn resolve_command(name: &OsStr, path_var: &str, pathext: Option<&str>) -> Option<PathBuf> {
    let name_str = name.to_string_lossy();

    // Names with path separators are relative/absolute — skip PATH search.
    if name_str.contains('/') || name_str.contains('\\') {
        return None;
    }

    let has_extension = Path::new(name).extension().is_some();

    for dir in path_var.split(';') {
        if dir.is_empty() {
            continue;
        }
        let dir = Path::new(dir);

        match pathext {
            // Mode A: PATHEXT unset — MSYS2 rules
            None => {
                // Exact match first (extensionless POSIX executables).
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
                // Then try .exe if name has no extension.
                if !has_extension {
                    let candidate = dir.join(format!("{name_str}.exe"));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
            // Mode B: PATHEXT set — cmd.exe rules
            Some(exts) => {
                if has_extension {
                    // Name already has an extension — exact match only.
                    let candidate = dir.join(name);
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                } else {
                    // Try each PATHEXT extension in order.
                    for ext in exts.split(';') {
                        if ext.is_empty() {
                            continue;
                        }
                        let candidate = dir.join(format!("{name_str}{ext}"));
                        if candidate.is_file() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }

    None
}
