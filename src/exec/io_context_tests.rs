use super::{CapturedIo, ProcessIo};

#[skuld::test]
fn process_io_context_has_streams() {
    let mut pio = ProcessIo::new();
    let _ctx = pio.context();
}

#[skuld::test]
fn captured_io_context_has_streams() {
    let mut cio = CapturedIo::new();
    let _ctx = cio.context();
}
