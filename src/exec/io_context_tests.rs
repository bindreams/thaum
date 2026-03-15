use super::{CapturedIo, ProcessIo};
use crate::exec::platform::is_fd_terminal;

#[skuld::test]
fn process_io_context_tty_flags_match_real_fds() {
    let mut pio = ProcessIo::new();
    let ctx = pio.context();
    assert_eq!(ctx.tty_stdout, is_fd_terminal(1));
    assert_eq!(ctx.tty_stderr, is_fd_terminal(2));
}

#[skuld::test]
fn captured_io_context_tty_flags_are_false() {
    let mut cio = CapturedIo::new();
    let ctx = cio.context();
    assert!(!ctx.tty_stdout, "CapturedIo should not claim stdout is a TTY");
    assert!(!ctx.tty_stderr, "CapturedIo should not claim stderr is a TTY");
}
