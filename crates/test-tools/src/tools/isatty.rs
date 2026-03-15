//! Reports whether stdin, stdout, and stderr are connected to a terminal.
//!
//! Prints `stdin:<yes|no>`, `stdout:<yes|no>`, and `stderr:<yes|no>` to stdout.

fn main() {
    use std::io::IsTerminal;
    let stdin_tty = std::io::stdin().is_terminal();
    let stdout_tty = std::io::stdout().is_terminal();
    let stderr_tty = std::io::stderr().is_terminal();
    println!("stdin:{}", if stdin_tty { "yes" } else { "no" });
    println!("stdout:{}", if stdout_tty { "yes" } else { "no" });
    println!("stderr:{}", if stderr_tty { "yes" } else { "no" });
}
