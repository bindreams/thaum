//! Minimal `writefd` — write a line to a raw file descriptor.
//!
//! A neutral observer for descriptor-inheritance tests: it reports whether a
//! given descriptor number is usable in this process without going through any
//! thaum code, so a test can distinguish "the parent closed it" from "the shell
//! changed its mind about resolving it".
//!
//! Borrows the descriptor and releases it again without closing, so the
//! caller's descriptor outlives the write.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("writefd: usage: writefd <fd> <text>");
        std::process::exit(2);
    }
    let fd: i32 = match args[0].parse() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("writefd: {}: {e}", args[0]);
            std::process::exit(2);
        }
    };

    match write_line(fd, &args[1]) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("writefd: {fd}: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(unix)]
fn write_line(fd: i32, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::fd::{FromRawFd, IntoRawFd};

    if fd < 0 {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    // SAFETY: `fd` is treated as borrowed — `into_raw_fd` below releases it
    // without closing, so this never invalidates the caller's descriptor.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let result = file.write_all(text.as_bytes()).and_then(|()| file.write_all(b"\n"));
    let _ = file.into_raw_fd();
    result
}

#[cfg(not(unix))]
fn write_line(_fd: i32, _text: &str) -> std::io::Result<()> {
    eprintln!("writefd: unsupported platform");
    std::process::exit(2);
}
