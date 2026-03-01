# Technical debt from the CI-fix / CommandEx session

Items discovered during the code review of commits `4ae8582..92db11f`.

## Critical

### PATH separator hardcoded as `:` on Windows
**Location:** `src/exec/special_builtins.rs:163`

`find_in_path()` splits `$PATH` on `:`, which is the Unix separator. Windows
uses `;`. This breaks `exec` and `source` path lookup on Windows.

**Fix:** `path_var.split(if cfg!(windows) { ';' } else { ':' })`.

### Handle leak on pipe creation error (Windows)
**Location:** `src/exec/command_ex/spawn_windows.rs:37-48`

If `make_inheritable(write_handle)?` fails after `create_pipe()` returned
successfully, the `read_handle` is never closed — it was not yet converted to a
`File` (which would close on drop).

**Fix:** Convert `read_handle` to `File` *before* calling `make_inheritable`,
so the `?` early-return drops the `File` and closes the handle.

### Unbounded pointer scan in CommandLineToArgvW round-trip
**Location:** `src/exec/command_ex.rs:258`

The wide-string length scan `(0..).take_while(|&j| *ptr.0.add(j) != 0)` has no
upper bound. A malformed result from `CommandLineToArgvW` (shouldn't happen in
practice, but this is `unsafe`) could read past the allocation.

**Fix:** Cap the loop: `(0..65536).take_while(...)`.

## Medium

### Null bytes in argv silently replaced with empty string
**Location:** `src/exec/command_ex.rs:304, 316`

`CString::new(a.as_bytes()).unwrap_or_else(|_| CString::new("").unwrap())`
silently replaces any argument containing a null byte with `""`. This alters
program semantics without reporting an error.

**Fix:** Propagate the `NulError` as `io::Error::new(InvalidInput, e)`.

### Pipeline builtin-via-`cat` hack
**Location:** `src/exec/pipeline.rs:126-158`

When a builtin runs in a pipeline and needs piped stdout, the code spawns `cat`
as a bridge process. This is:
- Non-portable (`cat` may not be in PATH on Windows or minimal systems)
- Inefficient (extra process for no reason)
- Fragile (`write_all` errors on line 135 are silently ignored)

**Fix:** Create an `os_pipe` pair, write the builtin's output to the write end,
pass the read end as the next stage's stdin. No child process needed.

### CWD save/restore is not RAII
**Location:** `src/exec/command_ex.rs:292-338`

The cwd is saved with `current_dir().ok()` (silently ignoring errors) and
restored with `let _ = set_current_dir(prev)` (also ignoring errors). If a
panic or early `?` return happens between save and restore, the cwd is not
restored.

**Fix:** Use an RAII guard struct whose `Drop` impl restores the cwd.

### WaitForSingleObject return value unchecked
**Location:** `src/exec/command_ex.rs:154`

`WaitForSingleObject(*handle, INFINITE)` can return `WAIT_FAILED`. The code
ignores the return value and proceeds directly to `GetExitCodeProcess`, which
would then report a stale or meaningless exit code.

**Fix:** Check that the return value is `WAIT_OBJECT_0`.

### CloseHandle errors silently ignored (Windows spawn)
**Location:** `src/exec/command_ex/spawn_windows.rs:114`

`let _ = unsafe { CloseHandle(handle) };` swallows errors. While a
`CloseHandle` failure is unlikely and non-fatal, it can indicate a
double-close bug.

**Fix:** At minimum `debug_assert!(result.is_ok())`.

## Low

### `commandline_posix` dead-code comment is misleading
**Location:** `src/exec/command_ex.rs:164`

The `#[allow(dead_code)]` comment says "will be used by Windows impl too", but
Windows uses `commandline_windows()` directly. The function is only called on
Unix.

**Fix:** Change the comment to "only used on Unix; tested via unit tests".

### Windows `-r` file permission check is a stub
**Location:** `src/exec/bash_test.rs:265`

The readable check (`_mask = 0o444`) returns `true` whenever the file exists,
without verifying read permission via ACLs.

**Fix:** Either check ACLs via `GetEffectiveRightsFromAclW` or document this as
an intentional simplification.

### Apple `getgroups` two-call pattern underdocumented
**Location:** `src/exec/environment.rs:303-318`

The code calls `getgroups(0, null)` to get the count, then `getgroups(n, buf)`
to get the values. This POSIX two-call idiom is subtle and deserves a comment.

### Docker env export allows shell expansion
**Location:** `tests/common/docker.rs:163`

Environment variable values are double-quoted (`export KEY="value"`), so `$VAR`
and backticks in values undergo shell expansion. This is documented as
intentional but is a security boundary if the corpus ever includes untrusted
test data.

## Deferred features

### Pipeline builtins need a proper pipe bridge
The `cat` hack in `pipeline.rs:126` should be replaced with a Rust pipe. This
is tracked as a TODO in the code.

### `exec` redirect-only mode
`exec 3>file` (without a command) should apply the redirect to the current
shell. The code at `special_builtins.rs:88` returns `Ok(0)` with a comment
"not yet fully implemented".

### Cross-platform FD3 inheritance test
`external_command_inherits_fd3` is `#[cfg(unix)]` because it uses `sh -c`.
Making it cross-platform requires either a Windows equivalent (`cmd /c` doesn't
support `>&3`) or teaching thaum's executor to resolve inherited OS file
descriptors for `>&N` redirects.

### `exec` on Windows should replace the process
`builtin_exec` currently spawns a child and waits (via `CommandEx::spawn`).
On Unix, the old code used `CommandExt::exec()` to replace the process image.
The new code falls back to spawn+wait on all platforms. The Unix path should
use `execvp` again for true process replacement.
