//! Shell impersonation: when the binary is invoked as `sh`, `bash`, or `dash`
//! (via symlink or argv[0] rename), it mimics that shell's CLI interface.

use std::path::Path;
use std::process;

use clap::Parser;

use super::{do_exec, CliArgs, PayloadFormat, Subcommand};

/// Which shell the binary is pretending to be (based on argv[0]).
#[derive(Clone, Copy)]
pub enum Impersonation {
    Sh,
    Bash,
    Dash,
}

/// Check whether the binary was invoked via a symlink named "sh", "bash", or "dash".
pub fn detect() -> Option<Impersonation> {
    let argv0 = std::env::args_os().next()?;
    let stem = Path::new(&argv0).file_stem()?.to_string_lossy();
    match stem.as_ref() {
        "sh" => Some(Impersonation::Sh),
        "bash" => Some(Impersonation::Bash),
        "dash" => Some(Impersonation::Dash),
        _ => None,
    }
}

// Clap definitions for impersonated CLIs ==============================================================================

/// POSIX-compatible shell CLI (sh, dash).
#[derive(Parser)]
#[command(disable_help_flag = true, disable_version_flag = true)]
struct PosixShellCli {
    /// Execute command string
    #[arg(short)]
    c: Option<String>,

    /// Read from stdin
    #[arg(short)]
    s: bool,

    /// Login shell
    #[arg(short)]
    l: bool,

    /// Interactive
    #[arg(short)]
    i: bool,

    /// Script file and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Bash-compatible CLI with GNU-style long options.
#[derive(Parser)]
#[command(disable_help_flag = true, disable_version_flag = true)]
struct BashCli {
    /// Execute command string
    #[arg(short)]
    c: Option<String>,

    /// Read from stdin
    #[arg(short)]
    s: bool,

    /// Login shell
    #[arg(short)]
    l: bool,

    /// Interactive
    #[arg(short)]
    i: bool,

    /// Show version and exit
    #[arg(long)]
    version: bool,

    /// Show help and exit
    #[arg(long)]
    help: bool,

    /// POSIX mode
    #[arg(long)]
    posix: bool,

    /// Skip ~/.bashrc
    #[arg(long)]
    norc: bool,

    /// Skip profile files
    #[arg(long)]
    noprofile: bool,

    /// Script file and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

// Dispatch ============================================================================================================

/// Run the shell in impersonation mode.
pub fn run(mode: Impersonation) {
    let dialect = match mode {
        Impersonation::Sh => thaum::Dialect::Posix,
        Impersonation::Bash => thaum::Dialect::Bash,
        Impersonation::Dash => thaum::Dialect::Dash,
    };

    let (dialect, c, file_and_args, force_interactive, login) = match mode {
        Impersonation::Bash => {
            let cli = BashCli::parse();
            if cli.version {
                print_bash_version(dialect);
                process::exit(0);
            }
            if cli.help {
                print_bash_help();
                process::exit(0);
            }
            let d = if cli.posix { thaum::Dialect::Posix } else { dialect };
            (d, cli.c, cli.args, cli.i, cli.l)
        }
        Impersonation::Sh | Impersonation::Dash => {
            let shell_name = match mode {
                Impersonation::Sh => "sh",
                _ => "dash",
            };
            // POSIX shells reject --long-options (except --).
            // Check before clap sees them so the error message matches real sh/dash.
            for arg in std::env::args().skip(1) {
                if arg == "--" {
                    break;
                }
                if arg.starts_with("--") {
                    eprintln!("{shell_name}: 0: Illegal option {arg}");
                    process::exit(2);
                }
                if !arg.starts_with('-') {
                    break;
                }
            }
            let cli = PosixShellCli::parse();
            (dialect, cli.c, cli.args, cli.i, cli.l)
        }
    };

    // No -c and no file args: check for interactive mode.
    if c.is_none() && file_and_args.is_empty() {
        let is_tty = super::is_stdin_terminal();
        if force_interactive || is_tty {
            super::interactive::run(dialect, login);
            return;
        }
    }

    let args = if let Some(cmd) = c {
        CliArgs {
            subcommand: Subcommand::Exec,
            dialect,
            verbose: false,
            quiet: false,
            command_str: Some(cmd),
            file_arg: None,
            script_args: file_and_args,
            payload_format: PayloadFormat::Json,
        }
    } else if let Some(file) = file_and_args.first() {
        CliArgs {
            subcommand: Subcommand::Exec,
            dialect,
            verbose: false,
            quiet: false,
            command_str: None,
            file_arg: Some(file.clone()),
            script_args: file_and_args[1..].to_vec(),
            payload_format: PayloadFormat::Json,
        }
    } else {
        // No args, not interactive — read from stdin as script.
        CliArgs {
            subcommand: Subcommand::Exec,
            dialect,
            verbose: false,
            quiet: false,
            command_str: None,
            file_arg: Some("-".to_string()),
            script_args: Vec::new(),
            payload_format: PayloadFormat::Json,
        }
    };

    do_exec(&args);
}

// Bash version/help output ============================================================================================

fn print_bash_version(dialect: thaum::Dialect) {
    let (major, minor, patch) = match dialect {
        thaum::Dialect::Bash44 => (4, 4, 0),
        thaum::Dialect::Bash50 => (5, 0, 0),
        _ => (5, 1, 0),
    };
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    println!("GNU bash, version {major}.{minor}.{patch}(1)-release ({arch}-pc-{os}-gnu)");
    println!("Imitated by Thaum! {}", random_sea_creature());
}

fn print_bash_help() {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    println!("GNU bash, version 5.1.0(1)-release-({arch}-pc-{os}-gnu)");
    println!("Usage:\tbash [GNU long option] [option] ...");
    println!("\tbash [GNU long option] [option] script-file ...");
    println!("GNU long options:");
    println!("\t--help");
    println!("\t--version");
    println!("Shell options:");
    println!("\t-c\tExecute command string");
    println!("\t-e\tExit on error");
    println!("\t-u\tTreat unset variables as errors");
    println!("\t-x\tPrint commands as they execute");
}

/// Pick a random sea creature emoji with weighted probabilities.
fn random_sea_creature() -> &'static str {
    let mut buf = [0u8; 2];
    getrandom::fill(&mut buf).unwrap_or_default();
    let val = u16::from_le_bytes(buf) as u32;
    // Scale to 0..1000.
    let n = val * 1000 / 65536;
    match n {
        0..929 => "\u{1f991}",   // 🦑 92.9%
        929..979 => "\u{1f419}", // 🐙 5.0%
        979..989 => "\u{1f990}", // 🦐 1.0%
        989..999 => "\u{1f99e}", // 🦞 1.0%
        999 => "\u{1f41a}",      // 🐚 0.1%
        _ => "\u{1f980}",        // 🦀 (remaining)
    }
}
