use super::{CapturedIo, ProcessIo};

#[skuld::test]
fn process_io_context_is_not_capturing() {
    let mut pio = ProcessIo::new();
    let ctx = pio.context();
    assert!(!ctx.capturing, "ProcessIo should produce a non-capturing IoContext");
}

#[skuld::test]
fn captured_io_context_is_capturing() {
    let mut cio = CapturedIo::new();
    let ctx = cio.context();
    assert!(ctx.capturing, "CapturedIo should produce a capturing IoContext");
}
