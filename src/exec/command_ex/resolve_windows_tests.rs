//! Unit tests for Windows command path resolution.

skuld::default_labels!(exec);

use std::ffi::OsStr;
use std::path::Path;

use skuld::temp_dir;

use super::resolve_windows::resolve_command;

/// Create a dummy file (empty) at the given path.
fn touch(path: &Path) {
    std::fs::write(path, b"").unwrap();
}

// Mode A: PATHEXT unset (MSYS2 rules) =================================================================================

#[skuld::test]
fn msys_extensionless_wins_over_exe(#[fixture(temp_dir)] dir: &Path) {
    touch(&dir.join("mytool"));
    touch(&dir.join("mytool.exe"));
    let path_var = dir.to_string_lossy();

    let result = resolve_command(OsStr::new("mytool"), &path_var, None);
    assert_eq!(result, Some(dir.join("mytool")));
}

#[skuld::test]
fn msys_exe_fallback(#[fixture(temp_dir)] dir: &Path) {
    touch(&dir.join("mytool.exe"));
    let path_var = dir.to_string_lossy();

    let result = resolve_command(OsStr::new("mytool"), &path_var, None);
    assert_eq!(result, Some(dir.join("mytool.exe")));
}

#[skuld::test]
fn msys_extensionless_only(#[fixture(temp_dir)] dir: &Path) {
    touch(&dir.join("mytool"));
    let path_var = dir.to_string_lossy();

    let result = resolve_command(OsStr::new("mytool"), &path_var, None);
    assert_eq!(result, Some(dir.join("mytool")));
}

#[skuld::test]
fn msys_path_order_dominates(#[fixture(temp_dir)] base: &Path) {
    let dir1 = base.join("dir1");
    let dir2 = base.join("dir2");
    std::fs::create_dir_all(&dir1).unwrap();
    std::fs::create_dir_all(&dir2).unwrap();
    touch(&dir1.join("mytool"));
    touch(&dir2.join("mytool.exe"));
    let path_var = format!("{};{}", dir1.display(), dir2.display());

    let result = resolve_command(OsStr::new("mytool"), &path_var, None);
    assert_eq!(result, Some(dir1.join("mytool")));
}

#[skuld::test]
fn msys_no_exe_append_when_name_has_extension(#[fixture(temp_dir)] dir: &Path) {
    // If the user types "mytool.bat", don't try "mytool.bat.exe"
    touch(&dir.join("mytool.bat"));
    let path_var = dir.to_string_lossy();

    let result = resolve_command(OsStr::new("mytool.bat"), &path_var, None);
    assert_eq!(result, Some(dir.join("mytool.bat")));
}

// Mode B: PATHEXT set (cmd.exe rules) =================================================================================

#[skuld::test]
fn cmd_pathext_order_within_one_dir(#[fixture(temp_dir)] dir: &Path) {
    touch(&dir.join("mytool.exe"));
    touch(&dir.join("mytool.bat"));
    let path_var = dir.to_string_lossy();

    // .exe before .bat → picks .exe
    let result = resolve_command(OsStr::new("mytool"), &path_var, Some(".exe;.bat"));
    assert_eq!(result, Some(dir.join("mytool.exe")));

    // Flip: .bat before .exe → picks .bat
    let result = resolve_command(OsStr::new("mytool"), &path_var, Some(".bat;.exe"));
    assert_eq!(result, Some(dir.join("mytool.bat")));
}

#[skuld::test]
fn cmd_path_order_beats_pathext_order(#[fixture(temp_dir)] base: &Path) {
    let dir1 = base.join("dir1");
    let dir2 = base.join("dir2");
    std::fs::create_dir_all(&dir1).unwrap();
    std::fs::create_dir_all(&dir2).unwrap();
    touch(&dir1.join("mytool.msc"));
    touch(&dir2.join("mytool.exe"));
    let path_var = format!("{};{}", dir1.display(), dir2.display());

    // Even though .exe is earlier in PATHEXT, dir1 is earlier in PATH → .msc wins
    let result = resolve_command(OsStr::new("mytool"), &path_var, Some(".exe;.msc"));
    assert_eq!(result, Some(dir1.join("mytool.msc")));
}

#[skuld::test]
fn cmd_extensionless_not_recognized(#[fixture(temp_dir)] dir: &Path) {
    touch(&dir.join("mytool"));
    touch(&dir.join("mytool.exe"));
    let path_var = dir.to_string_lossy();

    // In cmd mode, bare "mytool" (no ext) is NOT tried — only PATHEXT names
    let result = resolve_command(OsStr::new("mytool"), &path_var, Some(".exe"));
    assert_eq!(result, Some(dir.join("mytool.exe")));
}

#[skuld::test]
fn cmd_explicit_extension_found(#[fixture(temp_dir)] dir: &Path) {
    // User types "mytool.bat" explicitly — found even if .bat not in PATHEXT
    touch(&dir.join("mytool.bat"));
    let path_var = dir.to_string_lossy();

    let result = resolve_command(OsStr::new("mytool.bat"), &path_var, Some(".exe"));
    assert_eq!(result, Some(dir.join("mytool.bat")));
}

// Common ==============================================================================================================

#[skuld::test]
fn not_found_returns_none(#[fixture(temp_dir)] dir: &Path) {
    let path_var = dir.to_string_lossy();

    assert_eq!(resolve_command(OsStr::new("nonexistent"), &path_var, None), None);
    assert_eq!(
        resolve_command(OsStr::new("nonexistent"), &path_var, Some(".exe")),
        None
    );
}

#[skuld::test]
fn path_separator_skips_search(#[fixture(temp_dir)] dir: &Path) {
    touch(&dir.join("mytool.exe"));
    let path_var = dir.to_string_lossy();

    assert_eq!(resolve_command(OsStr::new("./mytool"), &path_var, None), None);
    assert_eq!(resolve_command(OsStr::new(".\\mytool"), &path_var, None), None);
    assert_eq!(
        resolve_command(OsStr::new("subdir/mytool"), &path_var, Some(".exe")),
        None
    );
}
