mod color;
mod error_fmt;
mod source_map;
mod yaml_writer;

use std::io::{self, Read};
use std::{env, fs, process};

use source_map::SourceMapper;
use yaml_writer::YamlWriter;

use shell_parser::exec::{ExecError, Executor};

#[derive(Clone, Copy, PartialEq)]
enum Subcommand {
    Lex,
    Parse,
    Exec,
}

struct CliArgs {
    subcommand: Subcommand,
    bash_mode: bool,
    /// Source from -c/--command <string>.
    command_str: Option<String>,
    /// File argument (filename or "-" for stdin).
    file_arg: Option<String>,
    /// Extra positional args after the file (exec only).
    script_args: Vec<String>,
}

fn parse_args(args: &[String]) -> CliArgs {
    let mut subcommand = None;
    let mut bash_mode = false;
    let mut command_str: Option<String> = None;
    let mut file_arg: Option<String> = None;
    let mut script_args: Vec<String> = Vec::new();
    let mut expect_command_value = false;

    for arg in &args[1..] {
        if expect_command_value {
            command_str = Some(arg.clone());
            expect_command_value = false;
            continue;
        }

        match arg.as_str() {
            "-h" | "--help" => {
                print_help(&args[0]);
                process::exit(2);
            }
            "--bash" => bash_mode = true,
            "-c" | "--command" => {
                expect_command_value = true;
            }
            "lex" if subcommand.is_none() && file_arg.is_none() => {
                subcommand = Some(Subcommand::Lex);
            }
            "parse" if subcommand.is_none() && file_arg.is_none() => {
                subcommand = Some(Subcommand::Parse);
            }
            "exec" if subcommand.is_none() && file_arg.is_none() => {
                subcommand = Some(Subcommand::Exec);
            }
            _ => {
                if file_arg.is_none() {
                    file_arg = Some(arg.clone());
                } else {
                    script_args.push(arg.clone());
                }
            }
        }
    }

    if expect_command_value {
        eprintln!("error: -c requires an argument");
        process::exit(2);
    }

    // Default subcommand is parse (backward compat)
    let subcommand = subcommand.unwrap_or(Subcommand::Parse);

    // Lex/Parse mode doesn't accept extra positional args
    if (subcommand == Subcommand::Parse || subcommand == Subcommand::Lex)
        && !script_args.is_empty()
    {
        eprintln!("error: unexpected argument '{}'", script_args[0]);
        process::exit(2);
    }

    // Must have either -c or a file argument
    if command_str.is_none() && file_arg.is_none() {
        print_help(&args[0]);
        process::exit(2);
    }

    // Can't have both -c and a file (for lex/parse)
    if command_str.is_some()
        && file_arg.is_some()
        && (subcommand == Subcommand::Parse || subcommand == Subcommand::Lex)
    {
        eprintln!("error: cannot use both -c and a file argument");
        process::exit(2);
    }

    // For exec with -c, the file_arg becomes the first script arg
    if command_str.is_some() && file_arg.is_some() && subcommand == Subcommand::Exec {
        script_args.insert(0, file_arg.take().unwrap());
    }

    CliArgs {
        subcommand,
        bash_mode,
        command_str,
        file_arg,
        script_args,
    }
}

/// Determine the source text and display filename from CLI args.
fn resolve_source(cli: &CliArgs) -> (String, String) {
    if let Some(cmd) = &cli.command_str {
        (cmd.clone(), "<command>".to_string())
    } else {
        let file_arg = cli.file_arg.as_deref().unwrap();
        let filename = if file_arg == "-" {
            "<stdin>".to_string()
        } else {
            file_arg.to_string()
        };
        let source = read_source(file_arg);
        (source, filename)
    }
}

pub fn run() {
    let args: Vec<String> = env::args().collect();
    let cli = parse_args(&args);

    match cli.subcommand {
        Subcommand::Lex => do_lex(&cli),
        Subcommand::Parse => do_parse(&cli),
        Subcommand::Exec => do_exec(&cli),
    }
}

fn do_lex(cli: &CliArgs) {
    use shell_parser::lexer::Lexer;
    use shell_parser::token::Token;

    let options = if cli.bash_mode {
        shell_parser::Dialect::Bash.options()
    } else {
        shell_parser::Dialect::Posix.options()
    };

    let (source, filename) = resolve_source(cli);
    let mapper = SourceMapper::new(&source);
    let mut lexer = Lexer::new(&source, options);

    let mut rows: Vec<(String, &'static str, String)> = Vec::new();

    loop {
        match lexer.next_token() {
            Ok(spanned) => {
                let (line, col) = mapper.offset_to_line_col(spanned.span.start.0);
                let location = format!("{}:{}:{}", filename, line, col);
                let name = spanned.token.token_name();
                if matches!(spanned.token, Token::Eof) {
                    break;
                }
                let text = match &spanned.token {
                    Token::Word(s) => s.clone(),
                    Token::IoNumber(n) => n.to_string(),
                    Token::HereDocBody(s) => {
                        let preview = s.replace('\n', "\\n");
                        if preview.len() > 40 {
                            format!("{}...", &preview[..37])
                        } else {
                            preview
                        }
                    }
                    Token::Newline => "\\n".to_string(),
                    _ => source[spanned.span.start.0..spanned.span.end.0].to_string(),
                };
                rows.push((location, name, text));
            }
            Err(e) => {
                let mapper = SourceMapper::new(&source);
                let parse_err = shell_parser::ParseError::from(e);
                error_fmt::print_error(&parse_err, &source, &filename, &mapper);
                process::exit(1);
            }
        }
    }

    // Compute column widths
    let loc_width = rows.iter().map(|(l, _, _)| l.len()).max().unwrap_or(8).max(8);
    let name_width = rows.iter().map(|(_, n, _)| n.len()).max().unwrap_or(5).max(5);

    // Print header
    println!(
        "{:<loc_w$}  {:<name_w$}  TEXT",
        "LOCATION",
        "TOKEN",
        loc_w = loc_width,
        name_w = name_width,
    );
    println!(
        "{:<loc_w$}  {:<name_w$}  ----",
        "--------",
        "-----",
        loc_w = loc_width,
        name_w = name_width,
    );

    for (location, name, text) in &rows {
        println!(
            "{:<loc_w$}  {:<name_w$}  {}",
            location,
            name,
            text,
            loc_w = loc_width,
            name_w = name_width,
        );
    }
}

fn do_parse(cli: &CliArgs) {
    let dialect = if cli.bash_mode {
        shell_parser::Dialect::Bash
    } else {
        shell_parser::Dialect::Posix
    };

    let (source, filename) = resolve_source(cli);
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

fn do_exec(cli: &CliArgs) {
    let dialect = if cli.bash_mode {
        shell_parser::Dialect::Bash
    } else {
        shell_parser::Dialect::Posix
    };

    let (source, filename) = resolve_source(cli);
    let mapper = SourceMapper::new(&source);

    let program = match shell_parser::parse_with(&source, dialect) {
        Ok(ast) => ast,
        Err(e) => {
            error_fmt::print_error(&e, &source, &filename, &mapper);
            process::exit(1);
        }
    };

    let mut executor = Executor::new();
    executor.env_mut().set_program_name(filename);
    executor
        .env_mut()
        .set_positional_params(cli.script_args.clone());

    match executor.execute(&program) {
        Ok(status) => process::exit(status),
        Err(ExecError::ExitRequested(code)) => process::exit(code),
        Err(ExecError::CommandNotFound(name)) => {
            eprintln!("{}: command not found", name);
            process::exit(127);
        }
        Err(e) => {
            error_fmt::print_exec_error(&e);
            process::exit(2);
        }
    }
}

fn read_source(file_arg: &str) -> String {
    if file_arg == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
            eprintln!("error reading stdin: {}", e);
            process::exit(1);
        });
        buf
    } else {
        fs::read_to_string(file_arg).unwrap_or_else(|e| {
            eprintln!("error reading '{}': {}", file_arg, e);
            process::exit(1);
        })
    }
}

fn print_help(program: &str) {
    eprintln!("Usage: {} [parse] [--bash] [-c <script>] <file>", program);
    eprintln!("       {} lex [--bash] [-c <script>] <file>", program);
    eprintln!("       {} exec [--bash] [-c <script>] <file> [args...]", program);
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  lex      Tokenize and display the token stream");
    eprintln!("  parse    Parse and display AST as YAML (default)");
    eprintln!("  exec     Execute the script");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --bash           Enable Bash dialect (default: POSIX)");
    eprintln!("  -c, --command    Read script from argument instead of file");
    eprintln!("  -h, --help       Show this help");
    eprintln!();
    eprintln!("Use - as <file> to read from stdin.");
}
