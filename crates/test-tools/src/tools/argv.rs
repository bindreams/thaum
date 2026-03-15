use std::io::{self, Write};

fn main() {
    let mut stdout = io::stdout().lock();
    let mut first = true;
    for arg in std::env::args() {
        if !first {
            stdout.write_all(b"\0").unwrap();
        }
        first = false;
        stdout.write_all(arg.as_bytes()).unwrap();
    }
}
