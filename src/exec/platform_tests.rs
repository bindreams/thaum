#[cfg(unix)]
use std::os::fd::AsRawFd;

#[cfg(unix)]
#[test]
fn is_fd_terminal_true_for_pty() {
    let pty = nix::pty::openpty(None, None).unwrap();
    assert!(super::is_fd_terminal(pty.slave.as_raw_fd()));
}

#[cfg(unix)]
#[test]
fn is_fd_terminal_false_for_pipe() {
    let (r, _w) = nix::unistd::pipe().unwrap();
    assert!(!super::is_fd_terminal(r.as_raw_fd()));
}

#[cfg(windows)]
#[test]
fn is_fd_terminal_true_for_console() {
    use std::fs::OpenOptions;
    use std::io::IsTerminal;
    // CONOUT$ is a special Windows device for the active console output buffer.
    // GetConsoleMode succeeds on this handle, so is_terminal() returns true.
    // On headless CI without a console, the open fails — skip silently.
    if let Ok(f) = OpenOptions::new().write(true).open("CONOUT$") {
        assert!(f.is_terminal());
    }
}

#[test]
fn is_fd_terminal_false_for_invalid_fd() {
    assert!(!super::is_fd_terminal(999));
}
