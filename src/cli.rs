//! CLI argument parsing (via clap) and dispatch to `lex`, `parse`, `exec`, and
//! `exec-ast` subcommands.

mod color;
mod error_fmt;

use std::io::{self, Read};
use std::{fs, process};

use clap::Parser;
use thaum::format::{SourceMapper, YamlWriter};

use thaum::exec::{ExecError, Executor, ProcessIo};

// Clap argument definitions ===========================================================================================

/// Shell script parser and executor
#[derive(Parser)]
#[command(name = "thaum")]
struct Cli {
    /// Enable Bash dialect (default: POSIX, alias for --bash51)
    #[arg(long, global = true)]
    bash: bool,

    /// Enable Bash 4.4 dialect
    #[arg(long, global = true)]
    bash44: bool,

    /// Enable Bash 5.0 dialect
    #[arg(long, global = true)]
    bash50: bool,

    /// Enable Bash 5.1 dialect
    #[arg(long, global = true)]
    bash51: bool,

    /// Suppress normal output (lex: skip table, parse: skip YAML). Errors still reported.
    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    subcmd: Option<CliCommand>,

    // Top-level args for the implicit "parse" default when no subcommand is given.
    /// Read script from argument instead of file
    #[arg(short, long = "command")]
    c: Option<String>,

    /// Script file (or "-" for stdin)
    file: Option<String>,
}

#[derive(clap::Subcommand)]
enum CliCommand {
    /// Tokenize and display the token stream
    Lex(SourceArgs),
    /// Parse and display AST as YAML (default when no subcommand given)
    Parse(SourceArgs),
    /// Execute the script
    Exec(ExecArgs),
    /// Execute a serialized AST from stdin (internal, used for subshells)
    #[command(hide = true)]
    ExecAst(ExecAstArgs),
}

#[derive(clap::Args)]
struct SourceArgs {
    /// Read script from argument instead of file
    #[arg(short, long = "command")]
    c: Option<String>,

    /// Emit all AST fields including defaults
    #[arg(long)]
    verbose: bool,

    /// Script file (or "-" for stdin)
    file: Option<String>,
}

#[derive(clap::Args)]
struct ExecArgs {
    /// Read script from argument instead of file
    #[arg(short, long = "command")]
    c: Option<String>,

    /// Script file (or "-" for stdin), followed by script arguments
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(clap::Args)]
struct ExecAstArgs {
    /// Payload format: json (default) or binary (bincode).
    #[arg(long, value_enum, default_value_t = PayloadFormat::Json)]
    format: PayloadFormat,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PayloadFormat {
    Json,
    Binary,
}

// Internal resolved args (kept from the original code) ================================================================

#[derive(Clone, Copy, PartialEq)]
enum Subcommand {
    Lex,
    Parse,
    Exec,
    ExecAst,
}

struct CliArgs {
    subcommand: Subcommand,
    dialect: thaum::Dialect,
    verbose: bool,
    quiet: bool,
    /// Source from -c/--command <string>.
    command_str: Option<String>,
    /// File argument (filename or "-" for stdin).
    file_arg: Option<String>,
    /// Extra positional args after the file (exec only).
    script_args: Vec<String>,
    /// Payload format for exec-ast (json or binary).
    payload_format: PayloadFormat,
}

impl CliArgs {
    fn dialect(&self) -> thaum::Dialect {
        self.dialect
    }
}

// Resolve clap output into CliArgs ====================================================================================

impl Cli {
    fn resolve(self) -> CliArgs {
        let dialect = resolve_dialect(self.bash, self.bash44, self.bash50, self.bash51);
        let quiet = self.quiet;
        let mut args = match self.subcmd {
            None => resolve_source(Subcommand::Parse, dialect, false, self.c, self.file),
            Some(CliCommand::Lex(a)) => resolve_source(Subcommand::Lex, dialect, a.verbose, a.c, a.file),
            Some(CliCommand::Parse(a)) => resolve_source(Subcommand::Parse, dialect, a.verbose, a.c, a.file),
            Some(CliCommand::Exec(a)) => resolve_exec(dialect, a.c, a.args),
            Some(CliCommand::ExecAst(a)) => CliArgs {
                subcommand: Subcommand::ExecAst,
                dialect,
                verbose: false,
                quiet: false,
                command_str: None,
                file_arg: None,
                script_args: Vec::new(),
                payload_format: a.format,
            },
        };
        args.quiet = quiet;
        args
    }
}

/// Resolve the dialect from mutually exclusive CLI flags. The most specific
/// versioned flag wins; `--bash` alone means latest (Bash51).
fn resolve_dialect(bash: bool, bash44: bool, bash50: bool, bash51: bool) -> thaum::Dialect {
    let count = [bash, bash44, bash50, bash51].iter().filter(|&&b| b).count();
    if count > 1 {
        eprintln!("error: specify at most one of --bash, --bash44, --bash50, --bash51");
        process::exit(2);
    }
    if bash44 {
        thaum::Dialect::Bash44
    } else if bash50 {
        thaum::Dialect::Bash50
    } else if bash51 || bash {
        thaum::Dialect::Bash
    } else {
        thaum::Dialect::Posix
    }
}

fn resolve_source(
    subcommand: Subcommand,
    dialect: thaum::Dialect,
    verbose: bool,
    c: Option<String>,
    file: Option<String>,
) -> CliArgs {
    if c.is_some() && file.is_some() {
        eprintln!("error: cannot use both -c and a file argument");
        process::exit(2);
    }
    if c.is_none() && file.is_none() {
        eprintln!("error: provide either -c <script> or a file argument");
        process::exit(2);
    }
    CliArgs {
        subcommand,
        dialect,
        verbose,
        quiet: false,
        command_str: c,
        file_arg: file,
        script_args: Vec::new(),
        payload_format: PayloadFormat::Json,
    }
}

fn resolve_exec(dialect: thaum::Dialect, c: Option<String>, args: Vec<String>) -> CliArgs {
    if c.is_none() && args.is_empty() {
        eprintln!("error: provide either -c <script> or a file argument");
        process::exit(2);
    }

    let (file_arg, script_args) = if c.is_some() {
        // With -c, all positional args are script args (no file needed).
        (None, args)
    } else {
        // First positional is the file, rest are script args.
        let mut args = args;
        let file = args.remove(0);
        (Some(file), args)
    };

    CliArgs {
        subcommand: Subcommand::Exec,
        dialect,
        verbose: false,
        quiet: false,
        command_str: c,
        file_arg,
        script_args,
        payload_format: PayloadFormat::Json,
    }
}

// Entry point =========================================================================================================

/// Determine the source text and display filename from CLI args.
fn load_source(cli: &CliArgs) -> (String, String) {
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

/// CLI entry point: parses clap args and dispatches to the selected subcommand.
pub fn run() {
    let cli = Cli::parse();
    let args = cli.resolve();

    match args.subcommand {
        Subcommand::Lex => do_lex(&args),
        Subcommand::Parse => do_parse(&args),
        Subcommand::Exec => do_exec(&args),
        Subcommand::ExecAst => do_exec_ast(&args),
    }
}

fn do_lex(cli: &CliArgs) {
    use thaum::lexer::Lexer;
    use thaum::token::{self, Token};

    let options = cli.dialect().options();
    let (source, filename) = load_source(cli);
    let mut lexer = Lexer::from_str(&source, options);

    if cli.quiet {
        loop {
            match lexer.next_token() {
                Ok(spanned) if spanned.token == Token::Eof => break,
                Ok(_) => {}
                Err(e) => {
                    let mapper = SourceMapper::new(&source);
                    let parse_err = thaum::ParseError::from(e);
                    error_fmt::print_error(&parse_err, &source, &filename, &mapper);
                    process::exit(1);
                }
            }
        }
        return;
    }

    let mapper = SourceMapper::new(&source);
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
                    Token::Literal(s) => s.clone(),
                    Token::SingleQuoted(s) => format!("'{}'", s),
                    Token::DoubleQuoted(s) => format!("\"{}\"", s),
                    Token::SimpleParam(s) => format!("${}", s),
                    Token::BraceParam(s) => format!("${{{}}}", s),
                    Token::CommandSub(s) => format!("$({})", s),
                    Token::BacktickSub(s) => format!("`{}`", s),
                    Token::ArithSub(s) => format!("$(({})))", s),
                    Token::Glob(k) => match k {
                        token::GlobKind::Star => "*".to_string(),
                        token::GlobKind::Question => "?".to_string(),
                        token::GlobKind::BracketOpen => "[".to_string(),
                    },
                    Token::TildePrefix(s) => format!("~{}", s),
                    Token::BashAnsiCQuoted(s) => format!("$'{}'", s),
                    Token::BashLocaleQuoted(s) => format!("$\"{}\"", s),
                    Token::BashExtGlob { kind, pattern } => {
                        let prefix = match kind {
                            token::ExtGlobTokenKind::ZeroOrOne => "?",
                            token::ExtGlobTokenKind::ZeroOrMore => "*",
                            token::ExtGlobTokenKind::OneOrMore => "+",
                            token::ExtGlobTokenKind::ExactlyOne => "@",
                            token::ExtGlobTokenKind::Not => "!",
                        };
                        format!("{}({})", prefix, pattern)
                    }
                    Token::BashProcessSub { direction, content } => {
                        format!("{}({})", direction, content)
                    }
                    Token::Whitespace => " ".to_string(),
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
                let parse_err = thaum::ParseError::from(e);
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
    let dialect = cli.dialect();
    let (source, filename) = load_source(cli);
    let mapper = SourceMapper::new(&source);

    let program = match thaum::parse_with(&source, dialect) {
        Ok(ast) => ast,
        Err(e) => {
            error_fmt::print_error(&e, &source, &filename, &mapper);
            process::exit(1);
        }
    };

    if cli.quiet {
        drop(program);
        return;
    }

    let w = if cli.verbose {
        YamlWriter::new_verbose(&mapper, &filename)
    } else {
        YamlWriter::new(&mapper, &filename)
    };
    let output = w.write_program(&program);
    if colored::control::SHOULD_COLORIZE.should_colorize() {
        color::print_colored_yaml(&output);
    } else {
        print!("{}", output);
    }
}

fn do_exec(cli: &CliArgs) {
    let dialect = cli.dialect();
    let options = dialect.options();
    let (source, filename) = load_source(cli);
    let mapper = SourceMapper::new(&source);

    let program = match thaum::parse_with(&source, dialect) {
        Ok(ast) => ast,
        Err(e) => {
            error_fmt::print_error(&e, &source, &filename, &mapper);
            process::exit(1);
        }
    };

    let mut executor = Executor::with_options(options);
    executor.env_mut().set_program_name(filename);
    executor.env_mut().set_positional_params(cli.script_args.clone());

    let mut process_io = ProcessIo::new();
    match executor.execute(&program, &mut process_io.context()) {
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

fn do_exec_ast(cli: &CliArgs) {
    use thaum::exec::environment::Environment;
    use thaum::exec::subshell::SubshellPayload;

    let payload: SubshellPayload = match cli.payload_format {
        PayloadFormat::Json => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).unwrap_or_else(|e| {
                eprintln!("exec-ast: error reading stdin: {e}");
                process::exit(2);
            });
            serde_json::from_str(&input).unwrap_or_else(|e| {
                eprintln!("exec-ast: invalid JSON payload: {e}");
                process::exit(2);
            })
        }
        PayloadFormat::Binary => {
            let stdin = io::stdin();
            bincode::deserialize_from(stdin.lock()).unwrap_or_else(|e| {
                eprintln!("exec-ast: invalid binary payload: {e}");
                process::exit(2);
            })
        }
    };

    let env = Environment::from_serialized(payload.env);
    let mut executor = Executor::with_env_and_options(env, payload.options);

    // Reconstruct fd_table from FDs inherited via CommandEx (posix_spawn).
    for fd in payload.inherited_fds {
        if let Some(file) = thaum::exec::redirect::dup_process_fd(fd) {
            executor.fd_table_mut().insert(fd, file);
        }
    }

    let mut process_io = ProcessIo::new();
    match executor.execute_lines(&payload.body, &mut process_io.context()) {
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
