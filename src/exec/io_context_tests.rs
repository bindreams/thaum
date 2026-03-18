use std::io::{Read, Write};

use super::{CapturedIo, IoContext};

skuld::default_labels!(exec);

// IoContext::from_process =============================================================================================

#[skuld::test]
fn process_io_has_fds_0_1_2() {
    let io = IoContext::from_process();
    assert!(io.fd(0).is_some(), "fd 0 (stdin) should be present");
    assert!(io.fd(1).is_some(), "fd 1 (stdout) should be present");
    assert!(io.fd(2).is_some(), "fd 2 (stderr) should be present");
}

// CapturedIo ==========================================================================================================

#[skuld::test]
fn captured_io_captures_stdout() {
    let (mut io, capture) = CapturedIo::new();
    io.fd_mut(1).unwrap().write_all(b"hello").unwrap();
    let output = capture.finish(io);
    assert_eq!(output.stdout_string(), "hello");
}

#[skuld::test]
fn captured_io_captures_stderr() {
    let (mut io, capture) = CapturedIo::new();
    io.fd_mut(2).unwrap().write_all(b"error msg").unwrap();
    let output = capture.finish(io);
    assert_eq!(output.stderr_string(), "error msg");
}

#[skuld::test]
fn captured_io_with_stdin() {
    let (mut io, capture) = CapturedIo::with_stdin(b"input data");
    let mut buf = Vec::new();
    io.fd_mut(0).unwrap().read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"input data");
    let output = capture.finish(io);
    assert_eq!(output.stdout_string(), "");
}

#[skuld::test]
fn captured_io_not_tty() {
    let (io, capture) = CapturedIo::new();
    assert!(!io.is_tty(0), "pipe stdin should not be a TTY");
    assert!(!io.is_tty(1), "pipe stdout should not be a TTY");
    assert!(!io.is_tty(2), "pipe stderr should not be a TTY");
    drop(capture.finish(io));
}

#[skuld::test]
fn captured_io_large_output_no_deadlock() {
    let (mut io, capture) = CapturedIo::new();
    // Write >64KB to both stdout and stderr to exceed the OS pipe buffer.
    // Drain threads run eagerly, so writes never block even from a single thread.
    let large_data = vec![b'x'; 128 * 1024];
    io.fd_mut(1).unwrap().write_all(&large_data).unwrap();
    io.fd_mut(2).unwrap().write_all(&large_data).unwrap();
    let output = capture.finish(io);
    assert_eq!(output.stdout_bytes().len(), 128 * 1024);
    assert_eq!(output.stderr_bytes().len(), 128 * 1024);
}

#[skuld::test]
fn captured_io_large_stdin_no_deadlock() {
    // Write >128KB to stdin — this exceeds the OS pipe buffer and previously
    // deadlocked because with_stdin wrote synchronously.
    let large_data = vec![b'y'; 128 * 1024];
    let (mut io, capture) = CapturedIo::with_stdin(&large_data);
    let mut buf = Vec::new();
    io.fd_mut(0).unwrap().read_to_end(&mut buf).unwrap();
    assert_eq!(buf.len(), 128 * 1024);
    let output = capture.finish(io);
    assert_eq!(output.stdout_string(), "");
}

// Save/restore ========================================================================================================

#[skuld::test]
fn io_context_save_and_restore() {
    let (mut io, capture) = CapturedIo::new();

    // Save fd 1 and replace with a new pipe.
    let (mut read_end, write_end) = crate::exec::pipeline::os_pipe().unwrap();
    let saved = io.save_and_set(1, write_end);
    assert!(saved.is_some(), "fd 1 existed before, should return Some");

    // Write to the new fd 1 (the redirect pipe).
    io.fd_mut(1).unwrap().write_all(b"redirected").unwrap();

    // Restore original fd 1. This drops the write_end (closing it), so
    // read_end will see EOF after "redirected".
    io.restore(1, saved);

    // Write to the restored fd 1 (the original capture pipe).
    io.fd_mut(1).unwrap().write_all(b"original").unwrap();

    // Read from the redirect pipe — write_end was closed by restore().
    let mut redirect_buf = Vec::new();
    read_end.read_to_end(&mut redirect_buf).unwrap();
    assert_eq!(redirect_buf, b"redirected");

    let output = capture.finish(io);
    assert_eq!(output.stdout_string(), "original");
}

#[skuld::test]
fn io_context_restore_removes_new_fd() {
    let (mut io, capture) = CapturedIo::new();

    // fd 5 doesn't exist yet.
    assert!(io.fd(5).is_none());

    let (_, write_end) = crate::exec::pipeline::os_pipe().unwrap();
    let saved = io.save_and_set(5, write_end);
    assert!(saved.is_none(), "fd 5 didn't exist before, should return None");

    // fd 5 now exists.
    assert!(io.fd(5).is_some());

    // Restore: saved=None means remove the fd.
    io.restore(5, saved);
    assert!(io.fd(5).is_none(), "fd 5 should be removed after restore(5, None)");

    drop(capture.finish(io));
}

// Uniform fd table ====================================================================================================

#[skuld::test]
fn io_context_uniform_fd_table() {
    let (mut io, capture) = CapturedIo::new();
    // fds 0, 1, 2 all in the same table.
    assert!(io.fd(0).is_some());
    assert!(io.fd(1).is_some());
    assert!(io.fd(2).is_some());

    // Can add higher fds identically.
    let (_, write_end) = crate::exec::pipeline::os_pipe().unwrap();
    io.set_fd(5, write_end);
    assert!(io.fd(5).is_some());

    drop(capture.finish(io));
}

#[skuld::test]
fn tty_override_cleared_on_redirect_and_restored() {
    let (mut io, capture) = CapturedIo::new();

    // Mark fd 1 as a TTY override.
    io.set_tty_override(1);
    assert!(io.is_tty(1), "fd 1 should report as TTY after override");

    // Simulate a redirect: replace fd 1 with a new pipe and clear tty override.
    let (_, write_end) = crate::exec::pipeline::os_pipe().unwrap();
    let saved = io.save_and_set(1, write_end);
    assert!(io.has_tty_override(1), "override still present before clear");
    io.clear_tty_override(1);
    assert!(!io.is_tty(1), "fd 1 should not report as TTY after clear");

    // Restore fd 1 and its tty override.
    io.restore(1, saved);
    io.set_tty_override(1);
    assert!(io.is_tty(1), "fd 1 should report as TTY after restore");

    drop(capture.finish(io));
}

#[skuld::test]
fn io_context_try_clone_fd() {
    let (io, capture) = CapturedIo::new();
    let cloned = io.try_clone_fd(1);
    assert!(cloned.is_ok(), "cloning fd 1 should succeed");
    let missing = io.try_clone_fd(99);
    assert!(missing.is_err(), "cloning non-existent fd should fail");
    // Drop cloned fd before finish() — otherwise its write-end keeps the pipe
    // open and drain blocks forever waiting for EOF.
    drop(cloned);
    drop(capture.finish(io));
}
