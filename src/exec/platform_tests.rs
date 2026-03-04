#[cfg(unix)]
use std::os::fd::AsRawFd;

skuld::default_labels!(exec);

#[cfg(unix)]
#[skuld::test]
fn is_fd_terminal_true_for_pty() {
    let pty = nix::pty::openpty(None, None).unwrap();
    assert!(super::is_fd_terminal(pty.slave.as_raw_fd()));
}

#[cfg(unix)]
#[skuld::test]
fn is_fd_terminal_false_for_pipe() {
    let (r, _w) = nix::unistd::pipe().unwrap();
    assert!(!super::is_fd_terminal(r.as_raw_fd()));
}

#[cfg(windows)]
#[skuld::test]
fn is_fd_terminal_false_for_pipe_stdout() {
    // Under nextest (or any piped context), stdout is not a terminal.
    assert!(!super::is_fd_terminal(1));
}

#[skuld::test]
fn is_fd_terminal_false_for_invalid_fd() {
    assert!(!super::is_fd_terminal(999));
}

#[skuld::test]
fn is_fd_terminal_false_for_negative_fd() {
    assert!(!super::is_fd_terminal(-1));
}

#[skuld::test]
fn is_fd_terminal_false_for_max_fd() {
    assert!(!super::is_fd_terminal(i32::MAX));
}
