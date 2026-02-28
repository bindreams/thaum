//! Shared helpers for conformance tests.

use crate::common::docker::DockerResult;
use thaum::exec::{CapturedIo, ExecError, Executor};

/// Find the thaum binary for subshell tests.
fn thaum_exe() -> std::path::PathBuf {
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("thaum");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

/// Run a script in our executor, capturing stdout.
pub fn run_ours(script: &str) -> DockerResult {
    let program = thaum::parse(script).unwrap_or_else(|e| panic!("parse failed for {:?}: {}", script, e));

    let mut executor = Executor::new();
    let _ = executor.env_mut().set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    executor.set_exe_path(thaum_exe());

    let mut captured = CapturedIo::new();
    let exit_code = match executor.execute(&program, &mut captured.context()) {
        Ok(status) => status,
        Err(ExecError::ExitRequested(code)) => code,
        Err(e) => panic!("exec failed for {:?}: {}", script, e),
    };

    DockerResult {
        stdout: captured.stdout_string(),
        stderr: captured.stderr_string(),
        exit_code,
    }
}

/// Run a script in a Docker container with the given shell.
fn run_in_docker(script: &str, shell: &str) -> DockerResult {
    crate::common::docker::run_in_reference_shell(script, shell)
}

/// Assert our executor produces the same exit code as both reference shells.
pub fn assert_exit_matches_both(script: &str) {
    let ours = run_ours(script);
    let dash = run_in_docker(script, "dash");
    let bash = run_in_docker(script, "bash-posix");

    assert_eq!(
        ours.exit_code, dash.exit_code,
        "Exit code mismatch (ours vs dash) for script: {:?}\n  ours={}\n  dash={}",
        script, ours.exit_code, dash.exit_code
    );
    assert_eq!(
        ours.exit_code, bash.exit_code,
        "Exit code mismatch (ours vs bash --posix) for script: {:?}\n  ours={}\n  bash={}",
        script, ours.exit_code, bash.exit_code
    );
}

/// Assert that the reference shells agree on stdout output for a script.
pub fn assert_shells_agree(script: &str) {
    let dash = run_in_docker(script, "dash");
    let bash = run_in_docker(script, "bash-posix");

    assert_eq!(
        dash.exit_code, bash.exit_code,
        "Exit code disagree (dash vs bash) for script: {:?}\n  dash={}\n  bash={}",
        script, dash.exit_code, bash.exit_code
    );
    assert_eq!(
        dash.stdout, bash.stdout,
        "Stdout disagree (dash vs bash) for script: {:?}\n  dash={:?}\n  bash={:?}",
        script, dash.stdout, bash.stdout
    );
}
