//! Minimal `cat` — copy files/stdin to stdout. Cross-platform test tool.

use std::io::{self, Read, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args == ["-"] {
        // Copy stdin to stdout.
        if let Err(e) = copy_stream(&mut io::stdin().lock(), &mut io::stdout().lock()) {
            eprintln!("cat: {e}");
            std::process::exit(1);
        }
    } else {
        for path in &args {
            if path == "-" {
                if let Err(e) = copy_stream(&mut io::stdin().lock(), &mut io::stdout().lock()) {
                    eprintln!("cat: {e}");
                    std::process::exit(1);
                }
            } else {
                match std::fs::File::open(path) {
                    Ok(mut f) => {
                        if let Err(e) = copy_stream(&mut f, &mut io::stdout().lock()) {
                            eprintln!("cat: {path}: {e}");
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("cat: {path}: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

fn copy_stream(reader: &mut dyn Read, writer: &mut dyn Write) -> io::Result<()> {
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
    }
    Ok(())
}
