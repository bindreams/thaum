//! Shell builtins that only need `Environment` (not the full `Executor`):
//! `echo`, `printf`, `true`, `false`, `exit`, `cd`, `export`, `unset`,
//! `read`, `shift`, `set`, `shopt`, `alias`, `test`/`[`, `declare`, etc.
//! Builtins requiring `Executor` access (`eval`, `source`, `exec`) live in
//! `special_builtins.rs` instead.

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
            | "getopts"
            | "pushd"
            | "popd"
            | "dirs"
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
        "readonly" => builtin_readonly(args, env, stdout),
        "local" => builtin_local(args, env),
        "declare" | "typeset" => builtin_declare(args, env, stdout, stderr),
        "getopts" => builtin_getopts(args, env, stderr),
        "pushd" => builtin_pushd(args, env, stdout, stderr),
        "popd" => builtin_popd(args, env, stdout, stderr),
        "dirs" => builtin_dirs(args, env, stdout, stderr),
        // eval, exec, source, and . are handled as special builtins in
        // execute_command (they need Executor access, not just Environment).
        // They should never reach run_builtin.
        "eval" | "exec" | "." | "source" => {
            debug_assert!(false, "{name} should be intercepted in execute_command");
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

fn builtin_printf(args: &[String], env: &mut Environment, stdout: &mut dyn Write) -> Result<i32, ExecError> {
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

    // Resolve LC_NUMERIC decimal separator for float formatting
    let locale = super::locale::numeric_locale(env);
    let decimal_sep = super::locale::decimal_separator(&locale);

    if let Some(vname) = var_name {
        // Write to buffer, then assign to variable
        let mut buf: Vec<u8> = Vec::new();
        let status = super::printf::printf_format(fmt_str, &remaining, &mut buf, decimal_sep, env);
        let output = String::from_utf8_lossy(&buf).into_owned();
        env.set_var(vname, &output)?;
        Ok(status)
    } else {
        let status = super::printf::printf_format(fmt_str, &remaining, stdout, decimal_sep, env);
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

fn builtin_cd(args: &[String], env: &mut Environment, stderr: &mut dyn Write) -> Result<i32, ExecError> {
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
            env.sync_dir_stack_cwd();
            Ok(0)
        }
        Err(e) => {
            let _ = writeln!(stderr, "cd: {e}");
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

fn builtin_alias(args: &[String], env: &mut Environment, stdout: &mut dyn Write) -> Result<i32, ExecError> {
    if args.is_empty() {
        // List all aliases
        let aliases = env.aliases();
        let mut names: Vec<_> = aliases.keys().collect();
        names.sort();
        for name in names {
            let value = &aliases[name];
            let _ = writeln!(stdout, "alias {name}='{value}'");
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
                    let _ = writeln!(stdout, "alias {arg}='{value}'");
                }
                None => {
                    status = 1;
                }
            }
        }
    }
    Ok(status)
}

fn builtin_unalias(args: &[String], env: &mut Environment, stderr: &mut dyn Write) -> Result<i32, ExecError> {
    if args.is_empty() {
        let _ = writeln!(stderr, "unalias: usage: unalias [-a] name [name ...]");
        return Ok(2);
    }

    let mut status = 0;
    for arg in args {
        if arg == "-a" {
            env.unalias_all();
        } else if !env.unalias(arg) {
            let _ = writeln!(stderr, "unalias: {arg}: not found");
            status = 1;
        }
    }
    Ok(status)
}

fn builtin_shopt(args: &[String], env: &mut Environment, stderr: &mut dyn Write) -> Result<i32, ExecError> {
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
    let _ = writeln!(stderr, "shopt: only 'shopt -s/-u expand_aliases' is supported");
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

fn builtin_shift(args: &[String], env: &mut Environment, stderr: &mut dyn Write) -> Result<i32, ExecError> {
    let n = if let Some(arg) = args.first() {
        match arg.parse::<usize>() {
            Ok(n) => n,
            Err(_) => {
                let _ = writeln!(stderr, "shift: {arg}: numeric argument required");
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

fn builtin_read(args: &[String], env: &mut Environment, stdin: &mut dyn Read) -> Result<i32, ExecError> {
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

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            // `set -- arg1 arg2 ...` sets positional parameters
            env.set_positional_params(args[i + 1..].to_vec());
            return Ok(0);
        } else if arg.starts_with('-') && arg.len() > 1 {
            // Parse -e, -u, -x, -eux, -o optname
            if arg == "-o" {
                i += 1;
                if i < args.len() {
                    match args[i].as_str() {
                        "errexit" => env.set_errexit(true),
                        "nounset" => env.set_nounset(true),
                        "xtrace" => env.set_xtrace(true),
                        _ => {}
                    }
                }
            } else {
                for ch in arg[1..].chars() {
                    match ch {
                        'e' => env.set_errexit(true),
                        'u' => env.set_nounset(true),
                        'x' => env.set_xtrace(true),
                        _ => {}
                    }
                }
            }
        } else if arg.starts_with('+') && arg.len() > 1 {
            // Parse +e, +u, +x etc. (disable)
            if arg == "+o" {
                i += 1;
                if i < args.len() {
                    match args[i].as_str() {
                        "errexit" => env.set_errexit(false),
                        "nounset" => env.set_nounset(false),
                        "xtrace" => env.set_xtrace(false),
                        _ => {}
                    }
                }
            } else {
                for ch in arg[1..].chars() {
                    match ch {
                        'e' => env.set_errexit(false),
                        'u' => env.set_nounset(false),
                        'x' => env.set_xtrace(false),
                        _ => {}
                    }
                }
            }
        } else {
            // Positional params (set arg1 arg2 ...)
            env.set_positional_params(args[i..].to_vec());
            return Ok(0);
        }
        i += 1;
    }
    Ok(0)
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
                        // Check PATHEXT on Windows for executable extensions.
                        let ext = std::path::Path::new(&args[1])
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("");
                        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
                        std::path::Path::new(&args[1]).exists()
                            && pathext
                                .split(';')
                                .any(|pe| pe.strip_prefix('.').is_some_and(|pe| pe.eq_ignore_ascii_case(ext)))
                    }
                }
                "-s" => std::fs::metadata(&args[1]).map(|m| m.len() > 0).unwrap_or(false),
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

fn builtin_readonly(args: &[String], env: &mut Environment, stdout: &mut dyn Write) -> Result<i32, ExecError> {
    if args.is_empty() {
        let mut vars = env.readonly_vars();
        vars.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, value) in &vars {
            let _ = writeln!(stdout, "declare -r {name}=\"{value}\"");
        }
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
    stderr: &mut dyn Write,
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
                    'n' => attrs.nameref_set = true,
                    'g' => attrs.global = true,
                    'p' => attrs.print = true,
                    'f' => attrs.list_functions = true,
                    'F' => attrs.list_function_names = true,
                    _ => {} // ignore unknown flags
                }
            }
        } else if arg.starts_with('+') && arg.len() > 1 && !arg.contains('=') {
            // Attribute removal flags (+x, +r, +i, +l, +u)
            for ch in arg[1..].chars() {
                match ch {
                    'x' => attrs.unexport = true,
                    'r' => attrs.unreadonly = true,
                    'i' => attrs.uninteger = true,
                    'l' => attrs.unlowercase = true,
                    'u' => attrs.unuppercase = true,
                    'a' | 'A' => {
                        // +a and +A cannot destroy array variables (bash behavior).
                        // Silently ignore.
                    }
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
            let mut names: Vec<String> = env.all_var_names();
            names.sort();
            for name in &names {
                if let Some(decl) = env.format_declare_p(name) {
                    let _ = writeln!(stdout, "{decl}");
                }
            }
        } else {
            for operand in &operands {
                let n = operand.split('=').next().unwrap_or(operand);
                if let Some(decl) = env.format_declare_p(n) {
                    let _ = writeln!(stdout, "{decl}");
                }
            }
        }
        return Ok(0);
    }

    // -F: list function names
    if attrs.list_function_names {
        let mut names = env.function_names();
        names.sort();
        for name in names {
            let _ = writeln!(stdout, "declare -f {name}");
        }
        return Ok(0);
    }
    // -f: list full function definitions
    if attrs.list_functions {
        let mut names = env.function_names();
        names.sort();
        for name in names {
            if let Some(func) = env.get_function(name) {
                let source = crate::format::SourceWriter::format_function(name, func);
                let _ = write!(stdout, "{source}");
            }
        }
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

        // Handle nameref (declare -n)
        if attrs.nameref_set {
            if let Some(ref target) = value {
                env.set_nameref(&name, target)?;
            } else {
                // declare -n var (no target) — create empty nameref
                env.set_nameref(&name, "")?;
            }
            continue;
        }

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

        // Evaluate value as arithmetic if the variable will have the integer
        // attribute AFTER this declare. Skip if +i is removing it.
        let will_be_integer = (attrs.integer_set || env.has_integer_attr(&name)) && !attrs.uninteger;
        let effective_value = if will_be_integer {
            value.map(|v| match crate::parser::arith_expr::parse_arith_expr(&v) {
                Ok(expr) => match crate::exec::arithmetic::evaluate_arith_expr(&expr, env) {
                    Ok(n) => n.to_string(),
                    Err(e) => {
                        let _ = writeln!(stderr, "declare: {v}: {e}");
                        "0".to_string()
                    }
                },
                Err(_) => {
                    let _ = writeln!(stderr, "declare: {v}: syntax error in expression");
                    "0".to_string()
                }
            })
        } else {
            value
        };

        // Scalar with attributes.
        env.declare_with_attrs(&name, effective_value.as_deref(), &attrs)?;
    }

    Ok(0)
}

// getopts builtin =====================================================================================================

/// `getopts optstring name [arg ...]`
///
/// Parses positional parameters (or explicit `arg` list) for options described
/// by `optstring`. Sets `name` to the found option character, updates `OPTIND`
/// and `OPTARG`. Returns 0 while options remain, >0 when done.
fn builtin_getopts(args: &[String], env: &mut Environment, stderr: &mut dyn Write) -> Result<i32, ExecError> {
    if args.len() < 2 {
        let _ = writeln!(stderr, "getopts: usage: getopts optstring name [arg ...]");
        return Ok(2);
    }

    let optstring = &args[0];
    let var_name = &args[1];

    // Determine the argument list to parse. Clone to avoid borrow conflicts.
    let explicit_args: Vec<String> = if args.len() > 2 {
        // Skip the `--` separator if present after name.
        let rest = &args[2..];
        if rest.first().map(|s| s.as_str()) == Some("--") {
            rest[1..].to_vec()
        } else {
            rest.to_vec()
        }
    } else {
        // Use positional parameters.
        env.positional_params().to_vec()
    };

    // Silent mode: leading ':' in optstring suppresses error messages.
    let silent = optstring.starts_with(':');
    let opts = if silent { &optstring[1..] } else { optstring.as_str() };

    // Read current OPTIND (1-based index into the argument list).
    let optind: usize = env.get_var("OPTIND").and_then(|s| s.parse().ok()).unwrap_or(1);

    let arg_idx = optind.saturating_sub(1); // 0-based index

    // Check if we've exhausted all arguments.
    if arg_idx >= explicit_args.len() {
        let _ = env.set_var(var_name, "?");
        return Ok(1);
    }

    let current_arg = &explicit_args[arg_idx];
    let subindex = env.getopts_subindex();

    // Not an option: doesn't start with '-', or is exactly '-'.
    if !current_arg.starts_with('-') || current_arg == "-" {
        let _ = env.set_var(var_name, "?");
        return Ok(1);
    }

    // '--' terminates option processing.
    if current_arg == "--" {
        let _ = env.set_var("OPTIND", &(optind + 1).to_string());
        let _ = env.set_var(var_name, "?");
        env.set_getopts_subindex(0);
        return Ok(1);
    }

    // Strip the leading '-' to get the option characters.
    let opt_chars: Vec<char> = current_arg[1..].chars().collect();
    let char_idx = subindex; // position within this grouped option string

    if char_idx >= opt_chars.len() {
        // All chars in this arg were consumed on previous calls — advance to next arg.
        // This shouldn't happen in normal flow (the last char sets OPTIND += 1),
        // but handle it defensively.
        let _ = env.set_var("OPTIND", &(optind + 1).to_string());
        env.set_getopts_subindex(0);
        let _ = env.set_var(var_name, "?");
        return Ok(1);
    }

    let opt_char = opt_chars[char_idx];

    // Look up this character in optstring.
    let opt_pos = opts.find(opt_char);
    let requires_arg = opt_pos
        .map(|pos| opts.get(pos + 1..pos + 2) == Some(":"))
        .unwrap_or(false);

    match opt_pos {
        None => {
            // Unknown option.
            if silent {
                let _ = env.set_var(var_name, "?");
                let _ = env.set_var("OPTARG", &opt_char.to_string());
            } else {
                let _ = writeln!(stderr, "getopts: illegal option -- {opt_char}");
                let _ = env.set_var(var_name, "?");
                env.unset_var("OPTARG").ok();
            }
            // Advance past this character.
            if char_idx + 1 < opt_chars.len() {
                env.set_getopts_subindex(char_idx + 1);
            } else {
                let _ = env.set_var("OPTIND", &(optind + 1).to_string());
                env.set_getopts_subindex(0);
            }
            Ok(0)
        }
        Some(_) if requires_arg => {
            // Option requires an argument.
            let _ = env.set_var(var_name, &opt_char.to_string());

            if char_idx + 1 < opt_chars.len() {
                // Remaining chars in this group become the argument.
                let optarg: String = opt_chars[char_idx + 1..].iter().collect();
                let _ = env.set_var("OPTARG", &optarg);
                let _ = env.set_var("OPTIND", &(optind + 1).to_string());
                env.set_getopts_subindex(0);
            } else if arg_idx + 1 < explicit_args.len() {
                // Next argument is the option-argument.
                let optarg = &explicit_args[arg_idx + 1];
                let _ = env.set_var("OPTARG", optarg);
                let _ = env.set_var("OPTIND", &(optind + 2).to_string());
                env.set_getopts_subindex(0);
            } else {
                // Missing argument.
                if silent {
                    let _ = env.set_var(var_name, ":");
                    let _ = env.set_var("OPTARG", &opt_char.to_string());
                } else {
                    let _ = writeln!(stderr, "getopts: option requires an argument -- {opt_char}");
                    let _ = env.set_var(var_name, "?");
                    env.unset_var("OPTARG").ok();
                }
                let _ = env.set_var("OPTIND", &(optind + 1).to_string());
                env.set_getopts_subindex(0);
            }
            Ok(0)
        }
        Some(_) => {
            // Regular option (no argument required).
            let _ = env.set_var(var_name, &opt_char.to_string());
            env.unset_var("OPTARG").ok();

            if char_idx + 1 < opt_chars.len() {
                // More characters in this grouped option.
                env.set_getopts_subindex(char_idx + 1);
            } else {
                // Move to next argument.
                let _ = env.set_var("OPTIND", &(optind + 1).to_string());
                env.set_getopts_subindex(0);
            }
            Ok(0)
        }
    }
}

// pushd/popd/dirs builtins ============================================================================================

/// Helper: change directory within the environment (used by pushd/popd).
fn do_cd(env: &mut Environment, dir: &std::path::Path, stderr: &mut dyn Write) -> Result<i32, ExecError> {
    let old_cwd = env.cwd().to_path_buf();
    match env.set_cwd(dir.to_path_buf()) {
        Ok(()) => {
            let _ = env.set_var("OLDPWD", &old_cwd.to_string_lossy());
            let pwd = env.cwd().to_string_lossy().into_owned();
            let _ = env.set_var("PWD", &pwd);
            env.sync_dir_stack_cwd();
            Ok(0)
        }
        Err(e) => {
            let _ = writeln!(stderr, "cd: {}: {e}", dir.display());
            Ok(1)
        }
    }
}

/// Print the directory stack (used by dirs and after pushd/popd).
fn print_dir_stack(env: &Environment, stdout: &mut dyn Write) {
    let home = env.get_var("HOME").map(|s| s.to_string());
    let parts: Vec<String> = env.dir_stack().iter().map(|p| format_dir(p, &home)).collect();
    let _ = writeln!(stdout, "{}", parts.join(" "));
}

fn builtin_pushd(
    args: &[String],
    env: &mut Environment,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, ExecError> {
    let mut no_cd = false;
    let mut operand: Option<&str> = None;

    for arg in args {
        if arg == "-n" {
            no_cd = true;
        } else {
            operand = Some(arg);
        }
    }

    match operand {
        None => {
            // pushd with no args: swap top two.
            if !env.dir_stack_swap() {
                let _ = writeln!(stderr, "pushd: no other directory");
                return Ok(1);
            }
            if !no_cd {
                let target = env.dir_stack()[0].clone();
                do_cd(env, &target, stderr)?;
            }
        }
        Some(dir) => {
            let path = if std::path::Path::new(dir).is_relative() {
                env.cwd().join(dir)
            } else {
                std::path::PathBuf::from(dir)
            };
            if no_cd {
                // Insert at position 1 without cd.
                env.dir_stack_insert(1, path);
            } else {
                env.dir_stack_push(path.clone());
                do_cd(env, &path, stderr)?;
            }
        }
    }

    print_dir_stack(env, stdout);
    Ok(0)
}

fn builtin_popd(
    args: &[String],
    env: &mut Environment,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, ExecError> {
    let no_cd = args.iter().any(|a| a == "-n");

    match env.dir_stack_pop() {
        Some(_popped) => {
            if !no_cd {
                let new_top = env.dir_stack()[0].clone();
                do_cd(env, &new_top, stderr)?;
            }
            print_dir_stack(env, stdout);
            Ok(0)
        }
        None => {
            let _ = writeln!(stderr, "popd: directory stack empty");
            Ok(1)
        }
    }
}

fn builtin_dirs(
    args: &[String],
    env: &mut Environment,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<i32, ExecError> {
    let clear = args.iter().any(|a| a == "-c");
    let long = args.iter().any(|a| a == "-l");
    let per_line = args.iter().any(|a| a == "-p");
    let verbose = args.iter().any(|a| a == "-v");

    if clear {
        env.dir_stack_clear();
        return Ok(0);
    }

    let home = if long {
        None
    } else {
        env.get_var("HOME").map(|s| s.to_string())
    };

    let stack = env.dir_stack();

    if verbose {
        for (i, p) in stack.iter().enumerate() {
            let s = format_dir(p, &home);
            let _ = writeln!(stdout, " {i}\t{s}");
        }
    } else if per_line {
        for p in stack {
            let s = format_dir(p, &home);
            let _ = writeln!(stdout, "{s}");
        }
    } else {
        let parts: Vec<String> = stack.iter().map(|p| format_dir(p, &home)).collect();
        let _ = writeln!(stdout, "{}", parts.join(" "));
    }

    Ok(0)
}

fn format_dir(path: &std::path::Path, home: &Option<String>) -> String {
    let s = path.to_string_lossy().to_string();
    if let Some(ref h) = home {
        if s == *h {
            return "~".to_string();
        }
        if let Some(rest) = s.strip_prefix(h.as_str()) {
            if rest.starts_with('/') {
                return format!("~{rest}");
            }
        }
    }
    s
}

#[cfg(test)]
#[path = "builtins_tests.rs"]
mod tests;
