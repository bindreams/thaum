//! Minimal `env` — print or modify environment. Cross-platform test tool.
//!
//! - `env` — print all environment variables as KEY=VALUE
//! - `env VAR=val ... cmd args` — run cmd with extra variables

use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        // Print all environment variables, sorted for deterministic output.
        let mut vars: Vec<(String, String)> = std::env::vars().collect();
        vars.sort();
        for (key, value) in vars {
            println!("{key}={value}");
        }
        return;
    }

    // Partition into VAR=VALUE assignments and command + args.
    let mut env_overrides = Vec::new();
    let mut cmd_start = args.len();
    for (i, arg) in args.iter().enumerate() {
        if let Some(eq_pos) = arg.find('=') {
            if eq_pos > 0 && arg[..eq_pos].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                env_overrides.push((&arg[..eq_pos], &arg[eq_pos + 1..]));
                continue;
            }
        }
        cmd_start = i;
        break;
    }

    if cmd_start >= args.len() {
        // No command — just print env with overrides applied.
        let mut vars: Vec<(String, String)> = std::env::vars().collect();
        for (key, value) in &env_overrides {
            if let Some(entry) = vars.iter_mut().find(|(k, _)| k == key) {
                entry.1 = value.to_string();
            } else {
                vars.push((key.to_string(), value.to_string()));
            }
        }
        vars.sort();
        for (key, value) in vars {
            println!("{key}={value}");
        }
        return;
    }

    // Run command with overrides.
    let mut cmd = Command::new(&args[cmd_start]);
    cmd.args(&args[cmd_start + 1..]);
    for (key, value) in &env_overrides {
        cmd.env(key, value);
    }

    match cmd.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("env: {}: {e}", args[cmd_start]);
            std::process::exit(127);
        }
    }
}
