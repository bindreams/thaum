//! Minimal `pwd` — print working directory. Cross-platform test tool.

fn main() {
    match std::env::current_dir() {
        Ok(dir) => println!("{}", dir.display()),
        Err(e) => {
            eprintln!("pwd: {e}");
            std::process::exit(1);
        }
    }
}
