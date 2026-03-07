//! Interactive shell mode: REPL loop with line editing, history, and prompt
//! expansion via rustyline.

use std::borrow::Cow;
use std::process;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Editor, Helper};

use thaum::exec::prompt::{self, PromptContext};
use thaum::exec::{ExecError, Executor, ProcessIo};
use thaum::interactive::is_incomplete;
use thaum::{Dialect, ShellOptions};

use super::error_fmt;

/// Rustyline helper: validates incomplete input and provides file-path completion.
struct ShellHelper {
    options: ShellOptions,
    file_completer: FilenameCompleter,
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.file_completer.complete(line, pos, _ctx)
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        Cow::Borrowed(prompt)
    }
}

impl Validator for ShellHelper {
    fn validate(&self, ctx: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();
        if is_incomplete(input, &self.options) {
            Ok(ValidationResult::Incomplete)
        } else {
            Ok(ValidationResult::Valid(None))
        }
    }
}

impl Helper for ShellHelper {}

/// Run the interactive REPL.
pub fn run(dialect: Dialect, _login: bool) {
    setup_signals();

    let options = dialect.options();
    let helper = ShellHelper {
        options: options.clone(),
        file_completer: FilenameCompleter::new(),
    };

    let config = rustyline::Config::builder()
        .auto_add_history(false) // We handle history filtering ourselves
        .build();
    let mut editor = Editor::<ShellHelper, DefaultHistory>::with_config(config).unwrap_or_else(|e| {
        eprintln!("thaum: failed to initialize line editor: {e}");
        process::exit(1);
    });
    editor.set_helper(Some(helper));

    let mut executor = Executor::with_options(options.clone());
    executor.env_mut().set_interactive(true);
    executor.env_mut().set_interactive_defaults(&options);
    executor.env_mut().inherit_from_process();

    let mut process_io = ProcessIo::new();

    source_startup_files(&mut executor, &mut process_io, &options, _login);

    // Load history from HISTFILE
    let histfile = resolve_histfile(&executor);
    if let Some(ref path) = histfile {
        let _ = editor.load_history(path.as_ref() as &std::path::Path);
    }

    let mut command_number: usize = 1;
    let mut prev_line: Option<String> = None;
    let mut eof_count: usize = 0;

    loop {
        // Execute PROMPT_COMMAND before displaying the prompt (Bash feature).
        if options.bash_prompt_escapes {
            execute_prompt_command(&mut executor, &mut process_io);
        }

        let ps1_template = executor.env().get_var("PS1").unwrap_or("$ ").to_string();
        let prompt_ctx = build_prompt_context(&executor, command_number);
        let prompt = prompt::expand_prompt_escapes(&ps1_template, &prompt_ctx, &options);

        match editor.readline(&prompt) {
            Ok(line) => {
                eof_count = 0;
                if line.trim().is_empty() {
                    continue;
                }

                let histcontrol = executor.env().get_var("HISTCONTROL").unwrap_or("").to_string();
                if thaum::interactive::should_save_to_history(&line, &histcontrol, prev_line.as_deref()) {
                    let _ = editor.add_history_entry(&line);
                }
                prev_line = Some(line.clone());

                // Expand and print PS0 if set (Bash 4.4+: post-input, pre-execution).
                if options.bash_prompt_escapes {
                    if let Some(ps0) = executor.env().get_var("PS0").map(|s| s.to_string()) {
                        let expanded = prompt::expand_prompt_escapes(&ps0, &prompt_ctx, &options);
                        eprint!("{expanded}");
                    }
                }

                match thaum::parse_with(&line, dialect) {
                    Ok(program) => {
                        match executor.execute(&program, &mut process_io.context()) {
                            Ok(_) => {}
                            Err(ExecError::ExitRequested(code)) => {
                                if let Some(ref path) = histfile {
                                    let _ = editor.save_history(path.as_ref() as &std::path::Path);
                                }
                                process::exit(code);
                            }
                            Err(ExecError::CommandNotFound(name)) => {
                                eprintln!("{name}: command not found");
                                executor.env_mut().set_last_exit_status(127);
                            }
                            Err(e) => {
                                error_fmt::print_exec_error(&e);
                                executor.env_mut().set_last_exit_status(2);
                            }
                        }
                        command_number += 1;
                    }
                    Err(e) => {
                        // In interactive mode, syntax errors don't exit the shell.
                        eprintln!("thaum: {e}");
                        executor.env_mut().set_last_exit_status(2);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: print newline and re-prompt
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: check ignoreeof
                let ignoreeof = executor
                    .env()
                    .get_var("IGNOREEOF")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);

                if ignoreeof > 0 && eof_count < ignoreeof {
                    eof_count += 1;
                    let remaining = ignoreeof - eof_count;
                    eprintln!("Use \"exit\" to leave the shell ({remaining} more Ctrl-D to force exit).");
                    continue;
                }

                if let Some(ref path) = histfile {
                    let _ = editor.save_history(path.as_ref() as &std::path::Path);
                }
                break;
            }
            Err(e) => {
                eprintln!("thaum: readline error: {e}");
                break;
            }
        }
    }
}

/// Build the prompt context from the executor's current state.
fn build_prompt_context(executor: &Executor, command_number: usize) -> PromptContext {
    let env = executor.env();

    #[cfg(unix)]
    let (username, hostname, uid) = {
        let uid = nix::unistd::getuid().as_raw();
        let username = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
            .ok()
            .flatten()
            .map(|u| u.name)
            .unwrap_or_else(|| uid.to_string());
        let hostname = nix::unistd::gethostname()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "localhost".into());
        (username, hostname, uid)
    };

    #[cfg(not(unix))]
    let (username, hostname, uid) = {
        let username = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
        let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "localhost".into());
        (username, hostname, 1000u32)
    };

    let cwd = env.cwd().to_string_lossy().into_owned();
    let home = env.get_var("HOME").unwrap_or("").to_string();
    let tty_name = tty_basename();

    PromptContext {
        username,
        hostname,
        cwd,
        home,
        shell_name: "thaum".into(),
        version: env!("CARGO_PKG_VERSION")
            .rsplit_once('.')
            .map(|(v, _)| v)
            .unwrap_or("0.1")
            .into(),
        version_patch: env!("CARGO_PKG_VERSION").into(),
        uid,
        history_number: 0, // TODO: wire to rustyline history length
        command_number,
        jobs_count: 0, // TODO: wire to job table
        tty_name,
    }
}

/// Get the terminal device basename (e.g. "3" from "/dev/pts/3").
fn tty_basename() -> String {
    #[cfg(unix)]
    {
        if let Ok(name) = nix::unistd::ttyname(std::io::stdin()) {
            return name.to_string_lossy().rsplit('/').next().unwrap_or("").to_string();
        }
    }
    String::new()
}

/// Resolve the history file path from HISTFILE or default to `~/.bash_history`.
fn resolve_histfile(executor: &Executor) -> Option<String> {
    if let Some(hf) = executor.env().get_var("HISTFILE") {
        if !hf.is_empty() {
            return Some(hf.to_string());
        }
    }
    let home = executor.env().get_var("HOME")?;
    Some(format!("{home}/.bash_history"))
}

/// Source startup files according to the shell dialect.
///
/// - **POSIX mode:** Source `$ENV` if set and readable.
/// - **Bash login shell:** `/etc/profile` → first of `~/.bash_profile`, `~/.bash_login`, `~/.profile`.
/// - **Bash non-login interactive:** `~/.bashrc`.
fn source_startup_files(executor: &mut Executor, io: &mut ProcessIo, options: &ShellOptions, login: bool) {
    if options.bash_prompt_escapes {
        // Bash mode
        if login {
            source_file_if_exists(executor, io, options, "/etc/profile");
            let home = executor.env().get_var("HOME").unwrap_or("").to_string();
            if !home.is_empty() {
                let found = source_file_if_exists(executor, io, options, &format!("{home}/.bash_profile"))
                    || source_file_if_exists(executor, io, options, &format!("{home}/.bash_login"))
                    || source_file_if_exists(executor, io, options, &format!("{home}/.profile"));
                let _ = found;
            }
        } else {
            let home = executor.env().get_var("HOME").unwrap_or("").to_string();
            if !home.is_empty() {
                source_file_if_exists(executor, io, options, &format!("{home}/.bashrc"));
            }
        }
    } else {
        // POSIX mode: source $ENV if set
        if let Some(env_file) = executor.env().get_var("ENV").map(|s| s.to_string()) {
            if !env_file.is_empty() {
                source_file_if_exists(executor, io, options, &env_file);
            }
        }
    }
}

/// Source a file if it exists. Returns `true` if the file was found and sourced.
fn source_file_if_exists(executor: &mut Executor, io: &mut ProcessIo, options: &ShellOptions, path: &str) -> bool {
    let Ok(source) = std::fs::read_to_string(path) else {
        return false;
    };
    if let Ok(program) = thaum::parse_with_options(&source, options.clone()) {
        let _ = executor.execute(&program, &mut io.context());
    }
    true
}

/// Execute the PROMPT_COMMAND variable if set.
fn execute_prompt_command(executor: &mut Executor, process_io: &mut ProcessIo) {
    let cmd = match executor.env().get_var("PROMPT_COMMAND") {
        Some(s) => s.to_string(),
        None => return,
    };

    if cmd.is_empty() {
        return;
    }

    // Parse and execute PROMPT_COMMAND. Errors are printed but don't affect the shell.
    if let Ok(program) = thaum::parse(&cmd) {
        let _ = executor.execute(&program, &mut process_io.context());
    }
}

/// Set up signal handling for interactive mode.
fn setup_signals() {
    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGQUIT, SIGTERM};
        // POSIX: interactive shells ignore SIGTERM and SIGQUIT.
        // SIGINT is handled by rustyline (returns Err(Interrupted)).
        let _ = signal_hook::flag::register(SIGTERM, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let _ = signal_hook::flag::register(SIGQUIT, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
    }
}
