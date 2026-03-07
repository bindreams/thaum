//! Minimal `touch` — create files or update timestamps. Cross-platform test tool.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("touch: missing file operand");
        std::process::exit(1);
    }
    for path in &args {
        if std::path::Path::new(path).exists() {
            // Update mtime by opening and closing.
            let _ = std::fs::OpenOptions::new().append(true).open(path);
        } else if let Err(e) = std::fs::File::create(path) {
            eprintln!("touch: cannot touch '{path}': {e}");
            std::process::exit(1);
        }
    }
}
