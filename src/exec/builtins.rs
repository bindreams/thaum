use std::io::{Read, Write};

use crate::exec::environment::Environment;
use crate::exec::error::ExecError;

/// Check if a command name is a built-in.
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "echo"
            | "printf"
            | "true"
            | "false"
            | "exit"
            | ":"
            | "cd"
            | "export"
            | "unset"
            | "return"
            | "break"
            | "continue"
            | "shift"
            | "read"
            | "eval"
            | "exec"
            | "."
            | "source"
            | "set"
            | "shopt"
            | "alias"
            | "unalias"
            | "test"
            | "["
            | "readonly"
            | "local"
            | "declare"
            | "typeset"
    )
}

/// Execute a built-in command.
///
/// Returns the exit status. Writes to `stdout`/`stderr` as needed.
pub fn run_builtin(
    name: &str,
    args: &[String],
    env: &mut Environment,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, ExecError> {
    match name {
        "echo" => builtin_echo(args, stdout),
        "printf" => builtin_printf(args, env, stdout),
        "true" | ":" => Ok(0),
        "false" => Ok(1),
        "exit" => builtin_exit(args),
        "cd" => builtin_cd(args, env, stderr),
        "export" => builtin_export(args, env),
        "unset" => builtin_unset(args, env),
        "return" => builtin_return(args),
        "break" => builtin_break(args),
        "continue" => builtin_continue(args),
        "shift" => builtin_shift(args, env, stderr),
        "read" => builtin_read(args, env, stdin),
        "set" => builtin_set(args, env),
        "shopt" => builtin_shopt(args, env, stderr),
        "alias" => builtin_alias(args, env, stdout),
        "unalias" => builtin_unalias(args, env, stderr),
        "test" | "[" => builtin_test(name, args, stderr),
        "readonly" => builtin_readonly(args, env),
        "local" => builtin_local(args, env),
        "declare" | "typeset" => builtin_declare(args, env, stdout),
        // eval, exec, source, and . are handled as special builtins in
        // execute_command (they need Executor access, not just Environment).
        // They should never reach run_builtin.
        "eval" | "exec" | "." | "source" => {
            debug_assert!(false, "{} should be intercepted in execute_command", name);
            Err(ExecError::CommandNotFound(name.to_string()))
        }
        _ => Err(ExecError::CommandNotFound(name.to_string())),
    }
}

fn builtin_echo(args: &[String], stdout: &mut dyn Write) -> Result<i32, ExecError> {
    // POSIX echo: no option parsing, just print args separated by spaces.
    // XSI extension: -n suppresses trailing newline.
    let (suppress_newline, start_idx) = if args.first().map(|s| s.as_str()) == Some("-n") {
        (true, 1)
    } else {
        (false, 0)
    };

    let output: Vec<&str> = args[start_idx..].iter().map(|s| s.as_str()).collect();
    write!(stdout, "{}", output.join(" ")).map_err(ExecError::Io)?;

    if !suppress_newline {
        writeln!(stdout).map_err(ExecError::Io)?;
    }

    Ok(0)
}

fn builtin_printf(
    args: &[String],
    env: &mut Environment,
    stdout: &mut dyn Write,
) -> Result<i32, ExecError> {
    if args.is_empty() {
        return Ok(0);
    }

    let mut arg_iter = args.iter();
    let mut var_name: Option<&str> = None;

    // Check for -v VAR option
    let first = arg_iter.next().unwrap();
    let fmt_str;

    if first == "-v" {
        match arg_iter.next() {
            Some(name) => var_name = Some(name.as_str()),
            None => return Ok(1), // -v without variable name
        }
        match arg_iter.next() {
            Some(f) => fmt_str = f.as_str(),
            None => return Ok(0), // -v VAR without format
        }
    } else {
        fmt_str = first.as_str();
    }

    let remaining: Vec<String> = arg_iter.cloned().collect();

    if let Some(vname) = var_name {
        // Write to buffer, then assign to variable
        let mut buf: Vec<u8> = Vec::new();
        let status = super::printf::printf_format(fmt_str, &remaining, &mut buf);
        let output = String::from_utf8_lossy(&buf).into_owned();
        env.set_var(vname, &output)?;
        Ok(status)
    } else {
        let status = super::printf::printf_format(fmt_str, &remaining, stdout);
        Ok(status)
    }
}

fn builtin_exit(args: &[String]) -> Result<i32, ExecError> {
    let code = if let Some(arg) = args.first() {
        arg.parse::<i32>().unwrap_or(2)
    } else {
        0
    };
    Err(ExecError::ExitRequested(code))
}

fn builtin_cd(
    args: &[String],
    env: &mut Environment,
    stderr: &mut dyn Write,
) -> Result<i32, ExecError> {
    let target = if let Some(dir) = args.first() {
        if dir == "-" {
            // cd - : go to previous directory ($OLDPWD)
            match env.get_var("OLDPWD") {
                Some(old) => std::path::PathBuf::from(old),
                None => {
                    let _ = writeln!(stderr, "cd: OLDPWD not set");
                    return Ok(1);
                }
            }
        } else {
            std::path::PathBuf::from(dir)
        }
    } else {
        // cd with no args: go to $HOME
        match env.get_var("HOME") {
            Some(home) => std::path::PathBuf::from(home),
            None => {
                let _ = writeln!(stderr, "cd: HOME not set");
                return Ok(1);
            }
        }
    };

    // Resolve relative paths against CWD.
    let resolved = if target.is_relative() {
        env.cwd().join(&target)
    } else {
        target
    };

    let old_cwd = env.cwd().to_path_buf();
    match env.set_cwd(resolved) {
        Ok(()) => {
            let _ = env.set_var("OLDPWD", &old_cwd.to_string_lossy());
            let pwd = env.cwd().to_string_lossy().into_owned();
            let _ = env.set_var("PWD", &pwd);
            Ok(0)
        }
        Err(e) => {
            let _ = writeln!(stderr, "cd: {}", e);
            Ok(1)
        }
    }
}

fn builtin_export(args: &[String], env: &mut Environment) -> Result<i32, ExecError> {
    for arg in args {
        if let Some((name, value)) = arg.split_once('=') {
            env.set_var(name, value)?;
            env.export_var(name);
        } else {
            env.export_var(arg);
        }
    }
    Ok(0)
}

fn builtin_unset(args: &[String], env: &mut Environment) -> Result<i32, ExecError> {
    for arg in args {
        // Skip -v (variable, default) and -f (function) flags
        if arg == "-v" || arg == "-f" {
            continue;
        }
        if let Some((base, subscript)) = super::expand::parse_array_subscript(arg) {
            if subscript == "@" || subscript == "*" {
                // unset a[@] / unset a[*] — unset the whole array
                env.unset_var(base)?;
            } else if env.is_assoc_array(base) {
                env.unset_assoc_element(base, subscript)?;
            } else {
                let index: usize = subscript.parse().unwrap_or(0);
                env.unset_array_element(base, index)?;
            }
        } else {
            env.unset_var(arg)?;
        }
    }
    Ok(0)
}

fn builtin_alias(
    args: &[String],
    env: &mut Environment,
    stdout: &mut dyn Write,
) -> Result<i32, ExecError> {
    if args.is_empty() {
        // List all aliases
        let aliases = env.aliases();
        let mut names: Vec<_> = aliases.keys().collect();
        names.sort();
        for name in names {
            let value = &aliases[name];
            let _ = writeln!(stdout, "alias {}='{}'", name, value);
        }
        return Ok(0);
    }

    let mut status = 0;
    for arg in args {
        if let Some((name, value)) = arg.split_once('=') {
            env.define_alias(name, value);
        } else {
            // Print a single alias
            match env.get_alias(arg) {
                Some(value) => {
                    let _ = writeln!(stdout, "alias {}='{}'", arg, value);
                }
                None => {
                    status = 1;
                }
            }
        }
    }
    Ok(status)
}

fn builtin_unalias(
    args: &[String],
    env: &mut Environment,
    stderr: &mut dyn Write,
) -> Result<i32, ExecError> {
    if args.is_empty() {
        let _ = writeln!(stderr, "unalias: usage: unalias [-a] name [name ...]");
        return Ok(2);
    }

    let mut status = 0;
    for arg in args {
        if arg == "-a" {
            env.unalias_all();
        } else if !env.unalias(arg) {
            let _ = writeln!(stderr, "unalias: {}: not found", arg);
            status = 1;
        }
    }
    Ok(status)
}

fn builtin_shopt(
    args: &[String],
    env: &mut Environment,
    stderr: &mut dyn Write,
) -> Result<i32, ExecError> {
    // Minimal shopt: only supports expand_aliases
    if args.len() == 2 {
        let flag = &args[0];
        let option = &args[1];
        if option == "expand_aliases" {
            match flag.as_str() {
                "-s" => {
                    env.set_expand_aliases(true);
                    return Ok(0);
                }
                "-u" => {
                    env.set_expand_aliases(false);
                    return Ok(0);
                }
                _ => {}
            }
        }
    }
    let _ = writeln!(
        stderr,
        "shopt: only 'shopt -s/-u expand_aliases' is supported"
    );
    Ok(1)
}

fn builtin_return(args: &[String]) -> Result<i32, ExecError> {
    let code = if let Some(arg) = args.first() {
        arg.parse::<i32>().unwrap_or(2)
    } else {
        0
    };
    Err(ExecError::ReturnRequested(code))
}

fn builtin_break(args: &[String]) -> Result<i32, ExecError> {
    let n = if let Some(arg) = args.first() {
        arg.parse::<usize>().unwrap_or(1).max(1)
    } else {
        1
    };
    Err(ExecError::BreakRequested(n))
}

fn builtin_continue(args: &[String]) -> Result<i32, ExecError> {
    let n = if let Some(arg) = args.first() {
        arg.parse::<usize>().unwrap_or(1).max(1)
    } else {
        1
    };
    Err(ExecError::ContinueRequested(n))
}

fn builtin_shift(
    args: &[String],
    env: &mut Environment,
    stderr: &mut dyn Write,
) -> Result<i32, ExecError> {
    let n = if let Some(arg) = args.first() {
        match arg.parse::<usize>() {
            Ok(n) => n,
            Err(_) => {
                let _ = writeln!(stderr, "shift: {}: numeric argument required", arg);
                return Ok(2);
            }
        }
    } else {
        1
    };

    let params = env.positional_params().to_vec();
    if n > params.len() {
        let _ = writeln!(stderr, "shift: shift count out of range");
        return Ok(1);
    }

    env.set_positional_params(params[n..].to_vec());
    Ok(0)
}

fn builtin_read(
    args: &[String],
    env: &mut Environment,
    stdin: &mut dyn Read,
) -> Result<i32, ExecError> {
    use std::io::BufRead;
    // Minimal `read VAR` implementation: read one line from stdin.
    let var_name = args.first().map(|s| s.as_str()).unwrap_or("REPLY");

    let mut reader = std::io::BufReader::new(stdin);
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => Ok(1), // EOF
        Ok(_) => {
            // Remove trailing newline
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            env.set_var(var_name, &line)?;
            Ok(0)
        }
        Err(e) => Err(ExecError::Io(e)),
    }
}

fn builtin_set(args: &[String], env: &mut Environment) -> Result<i32, ExecError> {
    if args.is_empty() {
        // `set` with no args: print all variables (simplified — just return 0)
        return Ok(0);
    }

    if args[0] == "--" {
        // `set -- arg1 arg2 ...` sets positional parameters
        env.set_positional_params(args[1..].to_vec());
        return Ok(0);
    }

    Err(ExecError::UnsupportedFeature(format!(
        "shell option: {}",
        args.join(" ")
    )))
}

fn builtin_test(name: &str, args: &[String], stderr: &mut dyn Write) -> Result<i32, ExecError> {
    // If invoked as `[`, the last arg must be `]`
    let args = if name == "[" {
        if args.last().map(|s| s.as_str()) != Some("]") {
            let _ = writeln!(stderr, "[: missing `]`");
            return Ok(2);
        }
        &args[..args.len() - 1]
    } else {
        args
    };

    let result = evaluate_test(args);
    Ok(if result { 0 } else { 1 })
}

/// Evaluate a POSIX test expression.
fn evaluate_test(args: &[String]) -> bool {
    match args.len() {
        0 => false,
        1 => {
            // `test STRING` — true if string is non-empty
            !args[0].is_empty()
        }
        2 => {
            // Unary operators
            match args[0].as_str() {
                "!" => !evaluate_test(&args[1..]),
                "-n" => !args[1].is_empty(),
                "-z" => args[1].is_empty(),
                "-e" => std::path::Path::new(&args[1]).exists(),
                "-f" => std::path::Path::new(&args[1]).is_file(),
                "-d" => std::path::Path::new(&args[1]).is_dir(),
                "-r" => {
                    // Readable check (simplified: just check existence)
                    std::path::Path::new(&args[1]).exists()
                }
                "-w" => std::path::Path::new(&args[1]).exists(),
                "-x" => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::metadata(&args[1])
                            .map(|m| m.permissions().mode() & 0o111 != 0)
                            .unwrap_or(false)
                    }
                    #[cfg(not(unix))]
                    {
                        std::path::Path::new(&args[1]).exists()
                    }
                }
                "-s" => std::fs::metadata(&args[1])
                    .map(|m| m.len() > 0)
                    .unwrap_or(false),
                "-L" | "-h" => std::fs::symlink_metadata(&args[1])
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false),
                _ => false,
            }
        }
        3 => {
            // Binary operators
            match args[1].as_str() {
                "=" | "==" => args[0] == args[2],
                "!=" => args[0] != args[2],
                "-eq" => parse_int(&args[0]) == parse_int(&args[2]),
                "-ne" => parse_int(&args[0]) != parse_int(&args[2]),
                "-lt" => parse_int(&args[0]) < parse_int(&args[2]),
                "-le" => parse_int(&args[0]) <= parse_int(&args[2]),
                "-gt" => parse_int(&args[0]) > parse_int(&args[2]),
                "-ge" => parse_int(&args[0]) >= parse_int(&args[2]),
                "-nt" => {
                    // File newer than
                    let a = std::fs::metadata(&args[0]).and_then(|m| m.modified()).ok();
                    let b = std::fs::metadata(&args[2]).and_then(|m| m.modified()).ok();
                    matches!((a, b), (Some(a), Some(b)) if a > b)
                }
                "-ot" => {
                    let a = std::fs::metadata(&args[0]).and_then(|m| m.modified()).ok();
                    let b = std::fs::metadata(&args[2]).and_then(|m| m.modified()).ok();
                    matches!((a, b), (Some(a), Some(b)) if a < b)
                }
                _ => {
                    // Unknown operator
                    false
                }
            }
        }
        4 => {
            // `! expr` with 3-arg expression
            if args[0] == "!" {
                !evaluate_test(&args[1..])
            } else {
                false
            }
        }
        _ => false,
    }
}

fn parse_int(s: &str) -> i64 {
    s.parse().unwrap_or(0)
}

fn builtin_readonly(args: &[String], env: &mut Environment) -> Result<i32, ExecError> {
    if args.is_empty() {
        // `readonly` with no args: list readonly vars (simplified — just return 0).
        // TODO: implement readonly variable listing
        return Ok(0);
    }

    for arg in args {
        if arg.starts_with('-') {
            continue; // Skip flags like -p
        }
        if let Some((name, value)) = arg.split_once('=') {
            env.set_var(name, value)?;
            env.set_readonly(name);
        } else {
            env.set_readonly(arg);
        }
    }
    Ok(0)
}

fn builtin_local(args: &[String], env: &mut Environment) -> Result<i32, ExecError> {
    if !env.in_function_scope() {
        return Err(ExecError::BadSubstitution(
            "local: can only be used in a function".to_string(),
        ));
    }

    for arg in args {
        if let Some((name, value)) = arg.split_once('=') {
            env.declare_local(name)?;
            env.set_var(name, value)?;
        } else {
            env.declare_local(arg)?;
            // If the variable doesn't exist yet, create it with empty value.
            if env.get_var(arg).is_none() {
                env.set_var(arg, "")?;
            }
        }
    }
    Ok(0)
}

/// Full `declare` / `typeset` builtin.
///
/// Supports flags: `-a` (indexed array), `-A` (associative array),
/// `-r` (readonly), `-x` (export), `-i` (integer), `-l` (lowercase),
/// `-u` (uppercase), `-g` (global), `-p` (print), `-f` / `-F` (functions).
fn builtin_declare(
    args: &[String],
    env: &mut Environment,
    stdout: &mut dyn Write,
) -> Result<i32, ExecError> {
    use crate::exec::environment::DeclareAttrs;

    let mut attrs = DeclareAttrs::default();
    let mut operands: Vec<String> = Vec::new();

    // Parse flags.  A flag argument starts with '-' and contains only flag
    // chars (no '=').  Everything else is an operand.
    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 && !arg.contains('=') {
            for ch in arg[1..].chars() {
                match ch {
                    'a' => attrs.indexed_array = true,
                    'A' => attrs.assoc_array = true,
                    'r' => attrs.readonly_set = true,
                    'x' => attrs.exported_set = true,
                    'i' => attrs.integer_set = true,
                    'l' => attrs.lowercase_set = true,
                    'u' => attrs.uppercase_set = true,
                    'g' => attrs.global = true,
                    'p' => attrs.print = true,
                    'f' => attrs.list_functions = true,
                    'F' => attrs.list_function_names = true,
                    _ => {} // ignore unknown flags
                }
            }
        } else {
            operands.push(arg.clone());
        }
    }

    // -p: print declarations
    if attrs.print {
        if operands.is_empty() {
            // Print all variables (sorted for determinism).
            let mut vars = env.all_vars();
            vars.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, value) in &vars {
                let _ = writeln!(stdout, "declare -- {}=\"{}\"", name, value);
            }
        } else {
            for operand in &operands {
                let n = operand.split('=').next().unwrap_or(operand);
                if let Some(val) = env.get_var(n) {
                    let _ = writeln!(stdout, "declare -- {}=\"{}\"", n, val);
                }
            }
        }
        return Ok(0);
    }

    // -f / -F: list functions (stub — no function source stored yet)
    if attrs.list_functions || attrs.list_function_names {
        // TODO: implement function listing when function source is stored
        return Ok(0);
    }

    // No operands: just list things (simplified — return 0)
    if operands.is_empty() {
        return Ok(0);
    }

    // Process each operand
    for operand in &operands {
        let (name, value) = if let Some((n, v)) = operand.split_once('=') {
            (n.to_string(), Some(v.to_string()))
        } else {
            (operand.clone(), None)
        };

        // Handle array creation
        if attrs.assoc_array {
            env.create_assoc(&name)?;
            // Apply other attributes (exported, etc.) if requested.
            env.declare_with_attrs(&name, None, &attrs)?;
            continue;
        }

        if attrs.indexed_array {
            if value.is_none() {
                // Create empty indexed array.
                env.set_array(&name, Vec::new())?;
            }
            // If value was provided the parser already handled
            // `declare -a a=(...)` as a normal array assignment on the
            // command, so the array is set before the builtin runs.
            env.declare_with_attrs(&name, None, &attrs)?;
            continue;
        }

        // Scalar with attributes.
        env.declare_with_attrs(&name, value.as_deref(), &attrs)?;
    }

    Ok(0)
}

#[cfg(test)]
#[path = "builtins_tests.rs"]
mod tests;
