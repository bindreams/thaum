//! Descriptor-state tests: what `exec` opens, what `>&-` closes, and what a
//! child is allowed to inherit.
//!
//! These tests are about the boundary between the shell's fd table and the
//! *host process's* fd table. thaum is embeddable, so a descriptor the host
//! opened must never receive a write the script directed elsewhere — and a
//! descriptor the script closed must not reappear through the host's table.
//!
//! Every expected value here was derived by running the equivalent script
//! under GNU bash 5.3, never by running thaum and recording what it did.
//!
//! **A descriptor under test is never obtained with `dup2` onto a fixed
//! number.** `HostFd` takes whatever number `into_raw_fd` hands back, which the
//! OS guarantees is unused. The test harness holds open descriptors of its own
//! — including skuld's cross-process coordination database — and stamping on a
//! literal fd 3 would corrupt them.

#![cfg(unix)]

use std::path::{Path, PathBuf};

use crate::*;

/// A real file, open on a real descriptor of *this* process, which the shell
/// under test never opened — the embedder's descriptor.
///
/// The number comes from `into_raw_fd`, so it is exclusively ours and no
/// concurrent test can be holding it.
struct HostFd {
    fd: i32,
    path: PathBuf,
}

impl HostFd {
    fn new(path: PathBuf) -> Self {
        use std::os::fd::{AsRawFd, IntoRawFd};
        let file = std::fs::File::create(&path).expect("create host fd probe file");

        // Rust opens files `O_CLOEXEC`, and `try_clone` preserves that. A
        // descriptor an embedder hands us is not close-on-exec — the one in the
        // original incident arrived through a shell redirect — so duplicate it
        // with a plain `dup(2)`, which POSIX defines as clearing FD_CLOEXEC.
        // Without this the probe vanishes at `exec` and every "the child could
        // not reach it" assertion passes for the wrong reason.
        let inheritable =
            thaum::exec::buffered_file::dup_process_fd(file.as_raw_fd()).expect("dup host fd probe descriptor");
        drop(file);

        let fd = inheritable.into_raw_fd();
        HostFd { fd, path }
    }

    /// What the host's file actually received. Empty means the escape is closed.
    fn contents(&self) -> String {
        std::fs::read_to_string(&self.path).expect("read host fd probe file")
    }
}

impl Drop for HostFd {
    fn drop(&mut self) {
        use std::os::fd::FromRawFd;
        // SAFETY: `self.fd` came from `into_raw_fd` and has not been closed.
        drop(unsafe { std::fs::File::from_raw_fd(self.fd) });
    }
}

fn shell_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Absolute path to a `test_tools` binary, for use inside a script.
///
/// Deliberately **not** invoked through `PATH`: thaum's external-command lookup
/// does not consult the shell's `PATH` variable (issue #15), so a bare name
/// would silently resolve against the process environment — or not at all.
fn tool(tools: &Path, name: &str) -> String {
    shell_path(&tools.join(name))
}

// Control: inherited descriptors are not the bug ======================================================================

#[skuld::test]
fn inherited_fd_is_writable_by_child(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // A child writing to a descriptor it legitimately inherited is ordinary
    // POSIX, and bash does exactly the same:
    //   bash -c 'sh -c "echo inherited >&3"; echo rc=$?' 3>probe
    //     -> rc=0, probe contains "inherited"
    // This must keep working. It is the behaviour issues #16 and #17 both
    // single out as *not* the defect.
    let probe = HostFd::new(dir.join("inherited.txt"));

    let script = format!("{} {} inherited; echo rc=$?", tool(tools, "writefd"), probe.fd);
    let r = exec!(&script);
    assert_eq!(
        r.stdout(),
        "rc=0\n",
        "child should be able to write to an inherited descriptor"
    );
    assert_eq!(probe.contents(), "inherited\n");
}

#[skuld::test]
fn inherited_fd_is_writable_by_shell(#[fixture(temp_dir)] dir: &Path) {
    // The shell's own redirection to an inherited descriptor also works in
    // bash: `bash -c 'echo self >&3' 3>probe` puts "self" in probe.
    let probe = HostFd::new(dir.join("shell-inherited.txt"));

    let script = format!("echo self >&{}", probe.fd);
    exec!(&script);
    assert_eq!(probe.contents(), "self\n");
}

#[skuld::test]
fn inherited_fd_survives_unrelated_close(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // Closing one descriptor must not disturb another.
    let probe = HostFd::new(dir.join("survives.txt"));
    let other = HostFd::new(dir.join("other.txt"));

    let script = format!(
        "exec {}>&-; {} {} inherited",
        other.fd,
        tool(tools, "writefd"),
        probe.fd
    );
    exec!(&script);
    assert_eq!(probe.contents(), "inherited\n");
    assert_eq!(other.contents(), "", "the closed descriptor must not receive anything");
}

#[skuld::test]
fn reopened_fd_is_inheritable_again(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // bash -c 'exec 3>&-; exec 3>r1; echo re >&3' leaves "re" in r1: a close
    // is not permanent, and reopening the number restores it for children too.
    let probe = HostFd::new(dir.join("reopened.txt"));
    let target = dir.join("reopened-target.txt");

    let script = format!(
        "exec {fd}>&-; exec {fd}>{target}; {wf} {fd} re",
        fd = probe.fd,
        target = shell_path(&target),
        wf = tool(tools, "writefd")
    );
    exec!(&script);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "re\n");
    assert_eq!(probe.contents(), "", "the host's file must not receive the write");
}

// `exec` must apply the redirections that follow its options (#16) ====================================================

#[skuld::test]
fn exec_dashdash_applies_redirections(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec -- 3>&1; echo hi 1>&3' 3>probe
    //   -> "hi" on stdout, probe empty.
    // thaum accepted the `--`, dropped `3>&1`, and the write landed in the
    // host's file. This is the case that overwrote a live SQLite database.
    let probe = HostFd::new(dir.join("dashdash.txt"));

    let script = format!("exec -- {fd}>&1; echo hi 1>&{fd}", fd = probe.fd);
    let r = exec!(&script);
    assert_eq!(
        r.stdout(),
        "hi\n",
        "`exec --` must apply the redirections that follow it"
    );
    assert_eq!(probe.contents(), "", "the host's descriptor must not receive the write");
}

#[skuld::test]
fn exec_dash_a_with_no_command_applies_redirections(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec -a foo 3>&1; echo hi >&3; echo rc=$?' 3>probe
    //   -> "hi", "rc=0", probe empty.
    // Same defect reached through a different option.
    let probe = HostFd::new(dir.join("dash-a.txt"));

    let script = format!("exec -a foo {fd}>&1; echo hi >&{fd}; echo rc=$?", fd = probe.fd);
    let r = exec!(&script);
    assert_eq!(r.stdout(), "hi\nrc=0\n");
    assert_eq!(probe.contents(), "");
}

#[skuld::test]
fn exec_dash_a_dashdash_with_no_command_applies_redirections(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec -a foo -- 3>&1; echo hi >&3' 3>probe -> "hi", probe empty.
    let probe = HostFd::new(dir.join("dash-a-dashdash.txt"));

    let script = format!("exec -a foo -- {fd}>&1; echo hi >&{fd}", fd = probe.fd);
    let r = exec!(&script);
    assert_eq!(r.stdout(), "hi\n");
    assert_eq!(probe.contents(), "");
}

#[skuld::test]
fn exec_dashdash_alone_is_a_noop() {
    // bash -c 'exec --; echo alive rc=$?' -> "alive rc=0". No command word, no
    // redirections: nothing to do, and the shell survives.
    let r = exec!("exec --; echo alive rc=$?");
    assert_eq!(r.stdout(), "alive rc=0\n");
}

#[skuld::test]
fn exec_dashdash_ends_option_parsing() {
    // bash -c 'exec -- -a 3>&1' -> "exec: -a: not found".
    // `--` must stop option parsing, so `-a` is a command name, not the
    // argv0-override flag. Getting this wrong would silently swallow the word.
    let r = exec!("exec -- -a", mode = ExecMode::Subprocess);
    assert_ne!(r.status(), 0, "`-a` after `--` is a command name, not a flag");
    assert!(
        r.stderr().contains("-a"),
        "expected `-a` to be reported as a command, got: {}",
        r.stderr()
    );
    r.stdout();
}

#[skuld::test]
fn exec_invalid_option_still_applies_redirections(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec -q 3>&1; echo after >&3; echo rc=$?' 3>probe
    //   -> stderr "exec: -q: invalid option" plus usage, "after" on stdout,
    //      "rc=0" (the rc of `echo`, not of `exec`), probe empty.
    // bash makes an `exec` redirection permanent even when the builtin then
    // rejects an option, which is also the fail-safe direction: the write goes
    // where the script asked rather than to the host.
    let probe = HostFd::new(dir.join("invalid-option.txt"));

    let script = format!("exec -q {fd}>&1; echo after >&{fd}; echo rc=$?", fd = probe.fd);
    let r = exec!(&script);
    assert_eq!(r.stdout(), "after\nrc=0\n");
    assert!(
        r.stderr().contains("invalid option"),
        "expected an invalid-option diagnostic, got: {}",
        r.stderr()
    );
    assert_eq!(probe.contents(), "");
}

// `>&-` must actually close, for the shell (#17) ======================================================================

#[skuld::test]
fn closed_fd_is_unreachable_from_shell(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec 3>&-; echo self >&3; echo rc=$?; echo alive' 3>probe
    //   -> "Bad file descriptor" on stderr, rc=1, "alive", probe empty.
    // The failure is per-command: the script keeps running.
    let probe = HostFd::new(dir.join("unreachable.txt"));

    let script = format!("exec {fd}>&-; echo self >&{fd}; echo rc=$?; echo alive", fd = probe.fd);
    let r = exec!(&script);
    assert_eq!(
        r.stdout(),
        "rc=1\nalive\n",
        "a closed descriptor must fail the command, not the script"
    );
    assert!(
        r.stderr().contains("bad file descriptor"),
        "expected a bad-descriptor diagnostic, got: {}",
        r.stderr()
    );
    assert_eq!(
        probe.contents(),
        "",
        "the host's descriptor must not be reachable after close"
    );
}

#[skuld::test]
fn closed_fd_is_unreachable_within_same_redirect_list(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec 3>f.txt; { echo x >&4; } 3>&- 4>&3; echo done'
    //   -> "3: Bad file descriptor" on stderr, "done" on stdout, f.txt empty.
    // A close earlier in the same redirect list must make the number
    // unresolvable to a later `>&N` in that list.
    let target = dir.join("same-list.txt");

    let script = format!(
        "exec 3>{t}; {{ echo x >&4; }} 3>&- 4>&3; echo done",
        t = shell_path(&target)
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "done\n");
    assert!(
        r.stderr().contains("bad file descriptor"),
        "expected a bad-descriptor diagnostic, got: {}",
        r.stderr()
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "");
    let _ = dir;
}

// Within one redirect list, the last word about a descriptor wins =====================================================

#[skuld::test]
fn redirect_list_reopen_after_close_wins(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec 3>f; echo first >&3; { echo second >&3; } 3>&- 3>g'
    //   -> f holds "first", g holds "second", rc 0.
    let f = dir.join("reopen-f.txt");
    let g = dir.join("reopen-g.txt");

    let script = format!(
        "exec 3>{f}; echo first >&3; {{ echo second >&3; }} 3>&- 3>{g}",
        f = shell_path(&f),
        g = shell_path(&g)
    );
    exec!(&script);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "first\n");
    assert_eq!(
        std::fs::read_to_string(&g).unwrap(),
        "second\n",
        "the later redirect must win"
    );
}

#[skuld::test]
fn redirect_list_close_after_reopen_wins(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec 3>f; { echo x >&3; } 3>g 3>&-; echo done'
    //   -> "3: Bad file descriptor" on stderr, "done" on stdout, both files
    //      empty (g is still created and truncated), rc 0.
    let f = dir.join("close-f.txt");
    let g = dir.join("close-g.txt");

    let script = format!(
        "exec 3>{f}; {{ echo x >&3; }} 3>{g} 3>&-; echo done",
        f = shell_path(&f),
        g = shell_path(&g)
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "done\n");
    assert!(
        r.stderr().contains("bad file descriptor"),
        "expected a bad-descriptor diagnostic, got: {}",
        r.stderr()
    );
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "");
    assert_eq!(std::fs::read_to_string(&g).unwrap(), "", "the later close must win");
}

// `>&-` must actually close, for children (#17) =======================================================================

#[skuld::test]
fn closed_fd_not_inherited_by_external(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // bash -c 'exec 3>&-; sh -c "echo closed >&3"; echo rc=$?' 3>probe
    //   -> "sh: 3: Bad file descriptor", rc=1, probe empty.
    let probe = HostFd::new(dir.join("external.txt"));

    let script = format!(
        "exec {fd}>&-; {wf} {fd} escaped; echo rc=$?",
        fd = probe.fd,
        wf = tool(tools, "writefd")
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "rc=1\n", "the child must fail on the closed descriptor");
    r.stderr();
    assert_eq!(
        probe.contents(),
        "",
        "the host's descriptor must not be inherited after close"
    );
}

#[skuld::test]
fn closed_fd_not_inherited_per_command(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // bash -c 'sh -c "echo x >&3" 3>&-; echo rc=$?' 3>probe
    //   -> "sh: 3: Bad file descriptor", rc=1, probe empty.
    // A per-command close, with no `exec` involved.
    let probe = HostFd::new(dir.join("per-command.txt"));

    let script = format!(
        "{wf} {fd} escaped {fd}>&-; echo rc=$?",
        fd = probe.fd,
        wf = tool(tools, "writefd")
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "rc=1\n");
    r.stderr();
    assert_eq!(probe.contents(), "");
}

#[skuld::test]
fn closed_fd_not_inherited_by_pipeline(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // bash -c 'exec 3>&-; sh -c "echo x >&3" | cat' 3>probe -> probe empty.
    let probe = HostFd::new(dir.join("pipeline.txt"));

    let script = format!(
        "exec {fd}>&-; {wf} {fd} escaped | {ct}",
        fd = probe.fd,
        wf = tool(tools, "writefd"),
        ct = tool(tools, "cat")
    );
    let r = exec!(&script);
    r.stdout();
    r.stderr();
    r.status();
    assert_eq!(
        probe.contents(),
        "",
        "a pipeline stage must not inherit a closed descriptor"
    );
}

#[skuld::test]
fn closed_fd_not_inherited_by_subshell(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // bash -c 'exec 3>&-; (sh -c "echo x >&3")' 3>probe -> probe empty.
    let probe = HostFd::new(dir.join("subshell.txt"));

    let script = format!(
        "exec {fd}>&-; ({wf} {fd} escaped)",
        fd = probe.fd,
        wf = tool(tools, "writefd")
    );
    let r = exec!(&script);
    r.stdout();
    r.stderr();
    r.status();
    assert_eq!(probe.contents(), "", "a subshell must not inherit a closed descriptor");
}

#[skuld::test]
fn closed_input_fd_not_inherited(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // The `<&-` spelling shares the path: bash -c 'exec 3<&-; sh -c "echo x >&3"'
    // 3>probe leaves probe empty and reports "Bad file descriptor".
    let probe = HostFd::new(dir.join("input-close.txt"));

    let script = format!(
        "exec {fd}<&-; {wf} {fd} escaped; echo rc=$?",
        fd = probe.fd,
        wf = tool(tools, "writefd")
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "rc=1\n");
    r.stderr();
    assert_eq!(probe.contents(), "");
}

#[skuld::test]
fn close_of_unopened_fd_does_not_break_spawn(#[fixture(temp_dir)] dir: &Path, #[fixture(test_tools)] tools: &Path) {
    // Closing a number the shell never opened must not make the next spawn
    // fail. On macOS a bare `posix_spawn_file_actions_addclose` against a
    // descriptor that is not open fails the whole spawn with EBADF, so this
    // pins the dup2-then-close construction. bash agrees the close is a no-op:
    //   bash -c 'exec 21>&-; echo ok'  ->  "ok"
    // The command must be an *external* one, since the risk is in the spawn.
    let marker = dir.join("spawn-marker.txt");
    std::fs::write(&marker, "ok\n").unwrap();

    let script = format!("exec 21>&-; {ct} {t}", ct = tool(tools, "cat"), t = shell_path(&marker));
    let r = exec!(&script);
    assert_eq!(
        r.stdout(),
        "ok\n",
        "a close of an unopened number must not break the next spawn"
    );
}

// Closed standard descriptors are closed in the child too (#41's child half) ==========================================

#[skuld::test]
fn closed_stdout_not_inherited_by_child(#[fixture(test_tools)] tools: &Path) {
    // bash -c 'sh -c "echo LEAKED" 1>&-' prints nothing: the child's stdout is
    // gone. thaum handed the child a pipe wired to the host's stdout instead,
    // so "LEAKED" came out — a write escape on descriptor 1.
    //
    // Only the absence of the leak is asserted, not the child's exit status.
    // The Rust runtime reopens /dev/null over any of fds 0-2 it finds closed at
    // startup, so a Rust observer cannot report EBADF on descriptor 1 and would
    // exit 0 either way. A C observer does see it: through the real binary,
    // `thaum exec -c 'sh -c "echo LEAKED" 1>&-; echo rc=$?'` gives
    // "echo: write error: Bad file descriptor" and rc=1, matching bash.
    let script = format!("{wf} 1 LEAKED 1>&-; echo done", wf = tool(tools, "writefd"));
    let r = exec!(&script);
    assert_eq!(r.stdout(), "done\n", "a closed stdout must not reach the host");
    r.stderr();
}

// Numeric-move redirections `N>&M-` / `N<&M-` (#17's audit) ===========================================================

#[skuld::test]
fn move_output_fd_closes_source(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec 5>f; echo hello5 >&5; exec 6>&5-; echo world5 >&5;
    //          echo world6 >&6; exec 6>&-; cat f'
    //   -> stderr "5: Bad file descriptor"; f holds "hello5\nworld6".
    // The move duplicates 5 onto 6 and then closes 5.
    let f = dir.join("move-out.txt");

    let script = format!(
        "exec 5>{f}; echo hello5 >&5; exec 6>&5-; echo world5 >&5; echo world6 >&6; exec 6>&-",
        f = shell_path(&f)
    );
    let r = exec!(&script);
    assert!(
        r.stderr().contains("bad file descriptor"),
        "the source descriptor must be closed by the move, got stderr: {}",
        r.stderr()
    );
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "hello5\nworld6\n");
}

#[skuld::test]
fn move_output_fd_source_becomes_unusable(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec 3>out; exec 4>&3-; echo moved >&4; echo after >&3; echo rc=$?'
    //   -> out holds "moved"; stderr "3: Bad file descriptor"; rc=1.
    // The destination takes over the file and the source is genuinely closed.
    let out = dir.join("move-source.txt");

    let script = format!(
        "exec 3>{out}; exec 4>&3-; echo moved >&4; echo after >&3; echo rc=$?",
        out = shell_path(&out)
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "rc=1\n", "writing to the moved-from descriptor must fail");
    assert!(
        r.stderr().contains("bad file descriptor"),
        "expected a bad-descriptor diagnostic, got: {}",
        r.stderr()
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "moved\n");
}

#[skuld::test]
fn move_input_fd_closes_source(#[fixture(temp_dir)] dir: &Path) {
    // bash -c 'exec 4<&3-; cat <&4' with 3 on a file reads the file, and
    // descriptor 3 is then closed:
    //   bash -c 'exec 4<&0-; cat <&4; sh -c "cat <&0"' < src
    //     -> "DATA", then "cat: stdin: Bad file descriptor".
    let src = dir.join("move-in.txt");
    std::fs::write(&src, "DATA\n").unwrap();

    let script = format!(
        "exec 3<{src}; exec 4<&3-; read line <&4; echo \"got=$line\"; read x <&3; echo rc=$?",
        src = shell_path(&src)
    );
    let r = exec!(&script);
    assert_eq!(
        r.stdout(),
        "got=DATA\nrc=1\n",
        "the move must transfer the read and close the source"
    );
    r.stderr();
}

#[skuld::test]
fn move_fd_does_not_leak_to_host(#[fixture(temp_dir)] dir: &Path) {
    // The move form failed with "ambiguous redirect" and applied nothing, so
    // the host's descriptor stayed live and received the write — the same
    // escape as #16 by another route.
    //   bash -c 'exec 4>tgt; exec N>&4-; echo moved >&N; echo after >&4; echo rc=$?' N>probe
    //     -> tgt holds "moved", probe empty, stderr "4: Bad file descriptor",
    //        rc=1. Measured identically for N = 3, 5, 9, 12 and 21, so the
    //        arbitrary number `HostFd` hands out is safe here.
    let probe = HostFd::new(dir.join("move-leak.txt"));
    // The scratch number the shell opens must not be a literal. `HostFd` takes
    // whatever the OS hands out, which can be any low number, so a literal `4`
    // here would collide with `probe.fd` on the runs where the OS picked 4.
    // Reserving a second descriptor makes the two numbers distinct by
    // construction rather than by luck.
    let scratch = HostFd::new(dir.join("move-leak-scratch.txt"));
    let target = dir.join("move-leak-target.txt");

    let script = format!(
        "exec {s}>{t}; exec {fd}>&{s}-; echo moved >&{fd}; echo after >&{s}; echo rc=$?",
        s = scratch.fd,
        t = shell_path(&target),
        fd = probe.fd
    );
    let r = exec!(&script);
    assert_eq!(r.stdout(), "rc=1\n");
    r.stderr();
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "moved\n",
        "the move must redirect to the source's file"
    );
    assert_eq!(probe.contents(), "", "the host's descriptor must not receive the write");
}

// Subprocess parity ===================================================================================================

#[skuld::test]
fn exec_dashdash_applies_redirections_in_subprocess(#[fixture(temp_dir)] dir: &Path) {
    // The same case through a real `thaum` process, which is how issue #16
    // reproduces it. Guards against the fix living only on the in-process path.
    let target = dir.join("subprocess-dashdash.txt");
    let script = format!("exec -- 3>{t}; echo hi 1>&3", t = shell_path(&target));
    exec!(&script, mode = ExecMode::Subprocess);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "hi\n");
}
