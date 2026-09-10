//! Tests for external command lookup against the shell's own `$PATH`.
//!
//! Every expected value here was measured against GNU bash 5.3.15 before the
//! test was written.
//!
//! Commands are identified by **exit status**: two `test_tools` binaries,
//! `true` (0) and `false` (1), are copied into different directories under the
//! same name, so "which file ran" is a single assertion that needs no shell
//! scripts, no output parsing, and no platform-specific fixtures. Neither
//! status collides with the 126 and 127 these tests also assert.

use std::path::{Path, PathBuf};

use crate::*;

/// Platform executable name. On Windows the planted file needs an extension;
/// with `PATHEXT` unset the resolver tries the bare name and then `.exe`, so
/// scripts can still invoke it as `mytool`.
fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Copy the `test_tools` binary `source` into `dir` under `name`.
///
/// A copy, not a link: each planted file must be a distinct executable so that
/// the directory it lives in is what decides which one runs.
fn plant(tools: &Path, source: &str, dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let dst = dir.join(exe(name));
    std::fs::copy(tools.join(exe(source)), &dst).unwrap_or_else(|e| panic!("copy {source} -> {}: {e}", dst.display()));
    dst
}

/// Render a path for embedding in a shell script.
fn s(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Separator between `PATH` entries. Windows uses `;` because `:` appears in
/// drive-letter paths.
fn sep() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

/// Join directories into a `PATH` value.
fn path_of(dirs: &[&Path]) -> String {
    dirs.iter().map(|d| s(d)).collect::<Vec<_>>().join(sep())
}

// Which binary the shell's PATH selects ===============================================================================

#[skuld::test]
fn lookup_shell_path_selects_between_same_named_binaries(
    #[fixture(test_tools)] tools: &Path,
    #[fixture(temp_dir)] dir: &Path,
) {
    let losing = dir.join("losing");
    let winning = dir.join("winning");
    plant(tools, "false", &losing, "mytool");
    plant(tools, "true", &winning, "mytool");

    // Both directories are on PATH; the earlier one must win, and the host
    // environment must not get a vote.
    let path = path_of(&[&winning, &losing]);
    let r = exec!("mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 0, "the first PATH entry's binary should run");

    let path = path_of(&[&losing, &winning]);
    let r = exec!("mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 1, "reordering PATH should change which binary runs");
}

#[skuld::test]
fn lookup_path_excluding_tool_makes_it_not_found(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let hidden = dir.join("hidden");
    let empty = dir.join("empty");
    plant(tools, "true", &hidden, "mytool");
    std::fs::create_dir_all(&empty).unwrap();

    // The tool exists, but not on the PATH the shell set. A shell that falls
    // back to the host environment would find it anyway; bash does not.
    let path = s(&empty);
    let r = exec!("mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 127);
    assert_eq!(r.stderr(), "mytool: command not found\n");
}

#[skuld::test]
fn lookup_missing_command_is_127(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("no_such_tool_xyz", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.status(), 127);
    assert_eq!(r.stderr(), "no_such_tool_xyz: command not found\n");
}

// Absent and empty PATH ===============================================================================================

// An empty `PATH` is a single empty entry, and an empty entry means the current
// directory (POSIX XBD 8.3); an unset `PATH` behaves identically. So the cwd is
// searched — but nothing else: no host `PATH`, and no `confstr(_CS_PATH)`
// default, which bash reserves for `command -p`.

#[skuld::test]
fn lookup_unset_path_searches_cwd(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let here = dir.join("here");
    plant(tools, "true", &here, "mytool");

    let script = format!("cd {}; unset PATH; mytool", s(&here));
    let start = s(dir);
    let r = exec!(&*script, env = &[("PATH", &*start)]);
    assert_eq!(r.status(), 0, "unset PATH should still resolve against the cwd");
}

#[skuld::test]
fn lookup_empty_path_searches_cwd(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let here = dir.join("here");
    plant(tools, "true", &here, "mytool");

    let script = format!("cd {}; PATH=; mytool", s(&here));
    let start = s(dir);
    let r = exec!(&*script, env = &[("PATH", &*start)]);
    assert_eq!(r.status(), 0, "empty PATH should still resolve against the cwd");
}

#[skuld::test]
fn lookup_unset_path_does_not_reach_beyond_cwd(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let bin = dir.join("bin");
    let here = dir.join("here");
    plant(tools, "true", &bin, "mytool");
    std::fs::create_dir_all(&here).unwrap();

    // `mytool` exists, but not in the cwd — and there is nowhere else to look.
    let script = format!("cd {}; unset PATH; mytool", s(&here));
    let start = s(&bin);
    let r = exec!(&*script, env = &[("PATH", &*start)]);
    assert_eq!(r.status(), 127);
    assert_eq!(r.stderr(), "mytool: command not found\n");
}

#[skuld::test]
fn lookup_empty_path_does_not_reach_beyond_cwd(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let bin = dir.join("bin");
    let here = dir.join("here");
    plant(tools, "true", &bin, "mytool");
    std::fs::create_dir_all(&here).unwrap();

    let script = format!("cd {}; PATH=; mytool", s(&here));
    let start = s(&bin);
    let r = exec!(&*script, env = &[("PATH", &*start)]);
    assert_eq!(r.status(), 127);
    assert_eq!(r.stderr(), "mytool: command not found\n");
}

// PATH need not be exported ===========================================================================================

#[skuld::test]
fn lookup_unexported_path_is_used(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let bin = dir.join("bin");
    plant(tools, "env", &bin, "showenv");

    // `exec!` sets PATH as a plain shell variable, so it is never exported and
    // never reaches the child's environment — yet lookup must still use it.
    let path = s(&bin);
    let r = exec!("showenv", env = &[("PATH", &*path)]);
    let out = r.stdout();
    assert!(
        !out.contains("PATH="),
        "PATH should not have been exported to the child, got: {out}"
    );
}

#[skuld::test]
fn lookup_prefix_assignment_applies_to_its_own_command(
    #[fixture(test_tools)] tools: &Path,
    #[fixture(temp_dir)] dir: &Path,
) {
    let shell_dir = dir.join("shell");
    let prefix_dir = dir.join("prefix");
    plant(tools, "false", &shell_dir, "mytool");
    plant(tools, "true", &prefix_dir, "mytool");

    // `PATH=<dir> mytool` uses <dir> to find `mytool` itself.
    let script = format!("PATH={} mytool", s(&prefix_dir));
    let path = s(&shell_dir);
    let r = exec!(&*script, env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 0);
}

// PATH entries resolved against the shell's cwd =======================================================================

#[skuld::test]
fn lookup_relative_path_entry_resolves_against_shell_cwd(
    #[fixture(test_tools)] tools: &Path,
    #[fixture(temp_dir)] dir: &Path,
) {
    let start = dir.join("start");
    let sibling = dir.join("sibling");
    std::fs::create_dir_all(&start).unwrap();
    plant(tools, "true", &sibling, "mytool");

    // `../sibling` is relative to the shell's cwd after `cd`, not to the cwd of
    // the process hosting the shell.
    let script = format!("cd {}; PATH=../sibling; mytool", s(&start));
    let empty = s(dir);
    let r = exec!(&*script, env = &[("PATH", &*empty)]);
    assert_eq!(r.status(), 0);
}

#[skuld::test]
fn lookup_empty_path_entry_means_cwd(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let here = dir.join("here");
    let elsewhere = dir.join("elsewhere");
    plant(tools, "true", &here, "mytool");
    std::fs::create_dir_all(&elsewhere).unwrap();

    // A leading empty entry means the shell's cwd.
    let script = format!("cd {}; PATH={}{}; mytool", s(&here), sep(), s(&elsewhere));
    let start = s(&elsewhere);
    let r = exec!(&*script, env = &[("PATH", &*start)]);
    assert_eq!(r.status(), 0);
}

// Unusable matches ====================================================================================================

#[cfg(unix)]
#[skuld::test]
fn lookup_non_executable_match_is_126(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    let planted = plant(tools, "true", &bin, "mytool");
    std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o644)).unwrap();

    let path = s(&bin);
    let r = exec!("mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 126);
    assert_eq!(r.stderr(), format!("{}: Permission denied\n", planted.display()));
}

#[cfg(unix)]
#[skuld::test]
fn lookup_search_continues_past_non_executable_match(
    #[fixture(test_tools)] tools: &Path,
    #[fixture(temp_dir)] dir: &Path,
) {
    use std::os::unix::fs::PermissionsExt;
    let unusable = dir.join("unusable");
    let usable = dir.join("usable");
    let planted = plant(tools, "false", &unusable, "mytool");
    std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o644)).unwrap();
    plant(tools, "true", &usable, "mytool");

    // A non-executable match does not end the search.
    let path = path_of(&[&unusable, &usable]);
    let r = exec!("mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 0);
}

#[skuld::test]
fn lookup_directory_named_like_command_is_skipped(
    #[fixture(test_tools)] tools: &Path,
    #[fixture(temp_dir)] dir: &Path,
) {
    let shadow = dir.join("shadow");
    let real = dir.join("real");
    std::fs::create_dir_all(shadow.join(exe("mytool"))).unwrap();
    plant(tools, "true", &real, "mytool");

    // Directories carry the execute bit but are never commands.
    let path = path_of(&[&shadow, &real]);
    let r = exec!("mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 0);

    let path = s(&shadow);
    let r = exec!("mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 127);
    assert_eq!(r.stderr(), "mytool: command not found\n");
}

// Every spawn site uses the same lookup ===============================================================================

#[skuld::test]
fn lookup_applies_in_pipeline_stage(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let chosen = dir.join("chosen");
    let decoy = dir.join("decoy");
    plant(tools, "true", &chosen, "mytool");
    plant(tools, "false", &decoy, "mytool");

    // A pipeline stage resolves through the same shell PATH as a plain command.
    // `mytool` is the last stage because a pipeline's status is the last
    // stage's; `echo` is a builtin and needs no lookup.
    let path = path_of(&[&chosen, &decoy]);
    let r = exec!("echo hi | mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 0);

    let path = path_of(&[&decoy, &chosen]);
    let r = exec!("echo hi | mytool", env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 1);
}

#[skuld::test]
fn lookup_pipeline_missing_command_is_127(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("echo hi | no_such_tool_xyz", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.status(), 127);
    assert_eq!(r.stderr(), "no_such_tool_xyz: command not found\n");
}

#[skuld::test]
fn lookup_applies_in_command_substitution(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let chosen = dir.join("chosen");
    let decoy = dir.join("decoy");
    plant(tools, "echo", &chosen, "mytool");
    plant(tools, "false", &decoy, "mytool");

    let path = path_of(&[&chosen, &decoy]);
    let r = exec!("echo \"[$(mytool picked)]\"", env = &[("PATH", &*path)]);
    assert_eq!(r.stdout(), "[picked]\n");
}

#[skuld::test]
fn lookup_command_substitution_missing_command_is_127(#[fixture(test_tools)] tools: &Path) {
    let tools_dir = tools.to_string_lossy();
    let r = exec!("x=$(no_such_tool_xyz); echo \"rc=$?\"", env = &[("PATH", &*tools_dir)]);
    assert_eq!(r.stdout(), "rc=127\n");
}

#[skuld::test]
fn lookup_applies_to_exec_builtin(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let chosen = dir.join("chosen");
    let decoy = dir.join("decoy");
    plant(tools, "true", &chosen, "mytool");
    plant(tools, "false", &decoy, "mytool");

    // Subprocess mode: `exec` replaces the process, which in-process mode
    // deliberately disallows.
    let path = path_of(&[&chosen, &decoy]);
    let r = exec!("exec mytool", mode = ExecMode::Subprocess, env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 0);

    let path = path_of(&[&decoy, &chosen]);
    let r = exec!("exec mytool", mode = ExecMode::Subprocess, env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 1);
}

#[skuld::test]
fn lookup_exec_builtin_missing_command_is_127(#[fixture(test_tools)] tools: &Path) {
    // bash: "exec: NAME: not found", distinct from ordinary lookup's wording.
    let tools_dir = tools.to_string_lossy();
    let r = exec!(
        "exec no_such_tool_xyz",
        mode = ExecMode::Subprocess,
        env = &[("PATH", &*tools_dir)]
    );
    assert_eq!(r.status(), 127);
    assert_eq!(r.stderr(), "exec: no_such_tool_xyz: not found\n");
}

// Names that are paths ================================================================================================

// A slash-containing name skips the search, so its 126 comes from the spawn
// errno rather than from lookup. Both routes must word it as bash does —
// otherwise the same condition reports two different ways depending on which
// path reached it.

#[cfg(unix)]
#[skuld::test]
fn lookup_slash_name_non_executable_is_126(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let sub = dir.join("sub");
    let planted = plant(tools, "true", &sub, "mytool");
    std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o644)).unwrap();

    let script = format!("cd {}; ./sub/{}", s(dir), exe("mytool"));
    let path = s(dir);
    let r = exec!(&*script, env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 126);
    assert_eq!(r.stderr(), format!("./sub/{}: Permission denied\n", exe("mytool")));
}

#[cfg(unix)]
#[skuld::test]
fn lookup_slash_name_non_executable_in_pipeline_is_126(
    #[fixture(test_tools)] tools: &Path,
    #[fixture(temp_dir)] dir: &Path,
) {
    use std::os::unix::fs::PermissionsExt;
    let sub = dir.join("sub");
    let planted = plant(tools, "true", &sub, "mytool");
    std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o644)).unwrap();

    let script = format!("cd {}; echo hi | ./sub/{}", s(dir), exe("mytool"));
    let path = s(dir);
    let r = exec!(&*script, env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 126);
    assert_eq!(r.stderr(), format!("./sub/{}: Permission denied\n", exe("mytool")));
}

#[skuld::test]
fn lookup_name_with_slash_ignores_path(#[fixture(test_tools)] tools: &Path, #[fixture(temp_dir)] dir: &Path) {
    let sub = dir.join("sub");
    let decoy = dir.join("decoy");
    plant(tools, "true", &sub, "mytool");
    plant(tools, "false", &decoy, "mytool");

    // A name containing a separator is not searched for; PATH is irrelevant.
    let script = format!("cd {}; ./sub/{}", s(dir), exe("mytool"));
    let path = s(&decoy);
    let r = exec!(&*script, env = &[("PATH", &*path)]);
    assert_eq!(r.status(), 0);
}
