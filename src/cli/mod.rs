mod color;
mod error_fmt;
mod source_map;
mod yaml_writer;

use std::io::{self, Read};
use std::{env, fs, process};

use source_map::SourceMapper;
use yaml_writer::YamlWriter;

pub fn run() {
    let args: Vec<String> = env::args().collect();

    // Parse flags and positional arg
    let mut bash_mode = false;
    let mut file_arg = None;

    for arg in &args[1..] {
        match arg.as_str() {
            "-h" | "--help" => {
                eprintln!("Usage: {} [--bash] <file>", args[0]);
                eprintln!("       {} [--bash] -    (read from stdin)", args[0]);
                process::exit(2);
            }
            "--bash" => bash_mode = true,
            _ => {
                if file_arg.is_some() {
                    eprintln!("error: unexpected argument '{}'", arg);
                    process::exit(2);
                }
                file_arg = Some(arg.clone());
            }
        }
    }

    let file_arg = match file_arg {
        Some(f) => f,
        None => {
            eprintln!("Usage: {} [--bash] <file>", args[0]);
            process::exit(2);
        }
    };

    let dialect = if bash_mode {
        shell_parser::Dialect::Bash
    } else {
        shell_parser::Dialect::Posix
    };

    let filename = if file_arg == "-" {
        "<stdin>".to_string()
    } else {
        file_arg.clone()
    };

    let source = if file_arg == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
            eprintln!("error reading stdin: {}", e);
            process::exit(1);
        });
        buf
    } else {
        fs::read_to_string(&file_arg).unwrap_or_else(|e| {
            eprintln!("error reading '{}': {}", file_arg, e);
            process::exit(1);
        })
    };

    let mapper = SourceMapper::new(&source);

    let program = match shell_parser::parse_with(&source, dialect) {
        Ok(ast) => ast,
        Err(e) => {
            error_fmt::print_error(&e, &source, &filename, &mapper);
            process::exit(1);
        }
    };
    let mut w = YamlWriter::new(&mapper, &filename);
    w.write_program(&program);

    let output = w.finish();
    if colored::control::SHOULD_COLORIZE.should_colorize() {
        color::print_colored_yaml(&output);
    } else {
        print!("{}", output);
    }
}
