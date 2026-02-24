//! Runtime evaluator for `[[ ]]` bash conditional expressions. Recursively
//! evaluates `BashTestExpr` nodes into `bool`, handling logical connectives,
//! string/integer comparisons, and file-test operators.

/// Evaluator for `[[ ]]` bash conditional expressions.
///
/// Recursively evaluates `BashTestExpr` AST nodes, returning `true`/`false`.
/// Mirrors the structure of `arithmetic.rs` (AST -> evaluation).
use crate::ast::{BashTestExpr, BinaryTestOp, UnaryTestOp};
use crate::exec::error::ExecError;
use crate::exec::io_context::IoContext;
use crate::exec::Executor;

/// Evaluate a `[[ ]]` conditional expression. Returns true/false.
///
/// The `_io` parameter is currently unused but reserved for future use
/// (e.g., command substitution inside test expressions that needs IO).
pub fn evaluate(expr: &BashTestExpr, executor: &mut Executor, _io: &mut IoContext<'_>) -> Result<bool, ExecError> {
    match expr {
        BashTestExpr::And { left, right } => {
            if !evaluate(left, executor, _io)? {
                Ok(false)
            } else {
                evaluate(right, executor, _io)
            }
        }
        BashTestExpr::Or { left, right } => {
            if evaluate(left, executor, _io)? {
                Ok(true)
            } else {
                evaluate(right, executor, _io)
            }
        }
        BashTestExpr::Not(inner) => Ok(!evaluate(inner, executor, _io)?),
        BashTestExpr::Group(inner) => evaluate(inner, executor, _io),
        BashTestExpr::Word(w) => {
            let s = executor.expand_word(w)?;
            Ok(!s.is_empty())
        }
        BashTestExpr::Unary { op, arg } => {
            let s = executor.expand_word(arg)?;
            Ok(evaluate_unary(*op, &s, executor.env()))
        }
        BashTestExpr::Binary { left, op, right } => {
            let l = executor.expand_word(left)?;
            let r = executor.expand_word(right)?;
            evaluate_binary(&l, *op, &r, executor.env_mut())
        }
    }
}

fn evaluate_unary(op: UnaryTestOp, s: &str, env: &crate::exec::Environment) -> bool {
    use std::path::Path;
    match op {
        // String tests
        UnaryTestOp::StringIsEmpty => s.is_empty(),
        UnaryTestOp::StringIsNonEmpty => !s.is_empty(),

        // File existence/type
        UnaryTestOp::FileExists => Path::new(s).exists(),
        UnaryTestOp::FileIsRegular => Path::new(s).is_file(),
        UnaryTestOp::FileIsDirectory => Path::new(s).is_dir(),
        UnaryTestOp::FileIsSymlink => std::fs::symlink_metadata(s)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        UnaryTestOp::FileHasSize => std::fs::metadata(s).map(|m| m.len() > 0).unwrap_or(false),

        // File permissions
        UnaryTestOp::FileIsReadable => file_permission_check(s, 0o444),
        UnaryTestOp::FileIsWritable => file_permission_check(s, 0o222),
        UnaryTestOp::FileIsExecutable => file_permission_check(s, 0o111),

        // Special file types (Unix-specific)
        UnaryTestOp::FileIsBlockDev
        | UnaryTestOp::FileIsCharDev
        | UnaryTestOp::FileIsPipe
        | UnaryTestOp::FileIsSocket => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                std::fs::metadata(s)
                    .map(|m| {
                        let ft = m.file_type();
                        match op {
                            UnaryTestOp::FileIsBlockDev => ft.is_block_device(),
                            UnaryTestOp::FileIsCharDev => ft.is_char_device(),
                            UnaryTestOp::FileIsPipe => ft.is_fifo(),
                            UnaryTestOp::FileIsSocket => ft.is_socket(),
                            _ => false,
                        }
                    })
                    .unwrap_or(false)
            }
            #[cfg(not(unix))]
            {
                false
            }
        }

        // Setuid/setgid/sticky bits (Unix-specific)
        UnaryTestOp::FileIsSetuid | UnaryTestOp::FileIsSetgid | UnaryTestOp::FileIsSticky => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::metadata(s)
                    .map(|m| {
                        let mode = m.permissions().mode();
                        match op {
                            UnaryTestOp::FileIsSetuid => mode & 0o4000 != 0,
                            UnaryTestOp::FileIsSetgid => mode & 0o2000 != 0,
                            UnaryTestOp::FileIsSticky => mode & 0o1000 != 0,
                            _ => false,
                        }
                    })
                    .unwrap_or(false)
            }
            #[cfg(not(unix))]
            {
                false
            }
        }

        // -O: file owned by effective user ID
        // TODO: proper implementation with libc::getuid() or nix crate;
        // approximated as "file exists" for now.
        UnaryTestOp::FileIsOwnedByUser => Path::new(s).exists(),

        // -G: file owned by effective group ID
        // TODO: proper implementation with libc::getgid() or nix crate;
        // approximated as "file exists" for now.
        UnaryTestOp::FileIsOwnedByGroup => Path::new(s).exists(),

        // -N: file modified since last read (mtime > atime)
        UnaryTestOp::FileModifiedSinceRead => std::fs::metadata(s)
            .map(|m| {
                let mtime = m.modified().ok();
                let atime = m.accessed().ok();
                matches!((mtime, atime), (Some(mt), Some(at)) if mt > at)
            })
            .unwrap_or(false),

        // -t FD: file descriptor is open and associated with a terminal
        // TODO: proper isatty check
        UnaryTestOp::FileDescriptorOpen => s.parse::<i32>().is_ok(),

        // -v: variable is set
        UnaryTestOp::VariableIsSet => env.get_var(s).is_some(),

        // -R: variable is a nameref (not implemented yet)
        UnaryTestOp::VariableIsNameRef => false,
    }
}

fn evaluate_binary(
    left: &str,
    op: BinaryTestOp,
    right: &str,
    env: &mut crate::exec::Environment,
) -> Result<bool, ExecError> {
    Ok(match op {
        // String/pattern comparison: RHS is treated as a glob pattern
        BinaryTestOp::StringEquals => {
            let locale = super::locale::ctype_locale(env);
            super::pattern::shell_pattern_match(left, right, &locale)
        }
        BinaryTestOp::StringNotEquals => {
            let locale = super::locale::ctype_locale(env);
            !super::pattern::shell_pattern_match(left, right, &locale)
        }
        BinaryTestOp::StringLessThan => {
            let locale = super::locale::collate_locale(env);
            super::locale::compare_strings(left, right, &locale) == std::cmp::Ordering::Less
        }
        BinaryTestOp::StringGreaterThan => {
            let locale = super::locale::collate_locale(env);
            super::locale::compare_strings(left, right, &locale) == std::cmp::Ordering::Greater
        }

        // Regex match
        BinaryTestOp::RegexMatch => regex_match(left, right, env)?,

        // Integer comparison
        BinaryTestOp::IntEq => parse_int(left) == parse_int(right),
        BinaryTestOp::IntNe => parse_int(left) != parse_int(right),
        BinaryTestOp::IntLt => parse_int(left) < parse_int(right),
        BinaryTestOp::IntLe => parse_int(left) <= parse_int(right),
        BinaryTestOp::IntGt => parse_int(left) > parse_int(right),
        BinaryTestOp::IntGe => parse_int(left) >= parse_int(right),

        // File comparison
        BinaryTestOp::FileNewerThan => {
            let a = std::fs::metadata(left).and_then(|m| m.modified()).ok();
            let b = std::fs::metadata(right).and_then(|m| m.modified()).ok();
            matches!((a, b), (Some(at), Some(bt)) if at > bt)
        }
        BinaryTestOp::FileOlderThan => {
            let a = std::fs::metadata(left).and_then(|m| m.modified()).ok();
            let b = std::fs::metadata(right).and_then(|m| m.modified()).ok();
            matches!((a, b), (Some(at), Some(bt)) if at < bt)
        }
        BinaryTestOp::FileSameDevice => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let a = std::fs::metadata(left).ok();
                let b = std::fs::metadata(right).ok();
                matches!(
                    (a, b),
                    (Some(ref am), Some(ref bm)) if am.dev() == bm.dev() && am.ino() == bm.ino()
                )
            }
            #[cfg(not(unix))]
            {
                false
            }
        }
    })
}

/// Match `text` against `pattern` as a POSIX extended regular expression.
/// On success, sets `BASH_REMATCH` array: `[0]` = full match, `[1..n]` = capture groups.
fn regex_match(text: &str, pattern: &str, env: &mut crate::exec::Environment) -> Result<bool, ExecError> {
    match regex::Regex::new(pattern) {
        Ok(re) => {
            if let Some(captures) = re.captures(text) {
                let mut rematch: Vec<String> = Vec::new();
                for cap in captures.iter() {
                    rematch.push(cap.map(|m| m.as_str().to_string()).unwrap_or_default());
                }
                // Set BASH_REMATCH as an indexed array
                let _ = env.set_array("BASH_REMATCH", rematch);
                Ok(true)
            } else {
                let _ = env.set_array("BASH_REMATCH", vec![]);
                Ok(false)
            }
        }
        Err(_) => {
            // Invalid regex -- return false (bash returns exit status 2)
            Ok(false)
        }
    }
}

fn parse_int(s: &str) -> i64 {
    crate::exec::numeric::parse_shell_int(s).unwrap_or(0)
}

fn file_permission_check(path: &str, _mask: u32) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & _mask != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        std::path::Path::new(path).exists()
    }
}
