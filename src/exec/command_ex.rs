//! Cross-platform process spawning with full fd table control.
//!
//! `CommandEx` is a plain data struct describing a child process to spawn.
//! `ChildEx` is the spawned process handle. `Fd` describes what to do with
//! each file descriptor. Platform-specific spawn logic uses `posix_spawnp`
//! on Unix and `CreateProcessW` on Windows.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::path::PathBuf;

// Fd ==============================================================================

/// What to do with a file descriptor in the child process.
pub(crate) enum Fd {
    /// Output pipe: child gets the write-end, parent gets the read-end
    /// via `ChildEx::take_pipe(fd)`. Used for capturing stdout/stderr.
    Pipe,
    /// Input pipe: child gets the read-end, parent gets the write-end
    /// via `ChildEx::take_pipe(fd)`. Used for feeding stdin to a child.
    InputPipe,
    /// Redirect to/from this file.
    File(File),
}

// CommandEx =======================================================================

/// Description of a child process to spawn. All fields are public; callers
/// build the struct, then consume it with `spawn(self)`.
pub(crate) struct CommandEx {
    /// Executable path for OS lookup (PATH search on both platforms).
    pub path: OsString,
    /// Full argv including [0]. Normally `argv[0] == path`.
    /// `exec -a name cmd` sets `argv[0] = "name"` while `path = "cmd"`.
    pub argv: Vec<OsString>,
    /// Environment variables. Populated from the current process at
    /// construction time; replace entirely to change.
    pub env: HashMap<OsString, OsString>,
    /// Working directory for the child. `None` = inherit parent's cwd.
    pub cwd: Option<PathBuf>,
    /// File descriptor table. Keys are fd numbers (0 = stdin, 1 = stdout, …).
    /// Fds not present here inherit from the parent process.
    pub fds: HashMap<i32, Fd>,
}

impl CommandEx {
    /// Create a new command from an argv vector. `path` defaults to `argv[0]`.
    /// Environment is inherited from the current process.
    #[contracts::debug_requires(!argv.is_empty(), "argv must have at least one element")]
    pub fn new(argv: Vec<OsString>) -> Self {
        let path = argv[0].clone();
        CommandEx {
            path,
            argv,
            env: std::env::vars_os().collect(),
            cwd: None,
            fds: HashMap::new(),
        }
    }

    /// Join `self.argv` into a single command-line string using platform
    /// quoting rules.
    ///
    /// - **Unix:** POSIX shell quoting (single-quote each arg, escape `'`).
    /// - **Windows:** MSVC CRT quoting (double-quote, escape `\` before `"`).
    ///   Includes a `debug_assert` round-trip via `CommandLineToArgvW`.
    #[allow(dead_code)] // Will be used by Windows CreateProcessW; tested via unit tests.
    pub fn commandline(&self) -> OsString {
        #[cfg(unix)]
        {
            commandline_posix(&self.argv)
        }
        #[cfg(windows)]
        {
            let result = commandline_windows(&self.argv);
            debug_assert_commandline_roundtrips(&result, &self.argv);
            result
        }
        #[cfg(not(any(unix, windows)))]
        {
            let mut s = OsString::new();
            for (i, arg) in self.argv.iter().enumerate() {
                if i > 0 {
                    s.push(" ");
                }
                s.push(arg);
            }
            s
        }
    }

    /// Spawn the child process. Consumes self.
    pub fn spawn(self) -> io::Result<ChildEx> {
        spawn_impl(self)
    }
}

// ChildEx =========================================================================

/// A spawned child process with optional pipe endpoints.
pub(crate) struct ChildEx {
    inner: ChildInner,
    /// Read-ends of pipes created for `Fd::Pipe` entries, keyed by fd number.
    pub pipes: HashMap<i32, File>,
}

#[allow(dead_code)] // Variants only constructed on their respective platform.
enum ChildInner {
    /// Already-finished pseudo-child (used for builtins in pipelines).
    Completed(i32),
    #[cfg(unix)]
    Pid(nix::libc::pid_t),
    #[cfg(windows)]
    Handle(windows::Win32::Foundation::HANDLE),
}

impl ChildEx {
    /// Create a `ChildEx` that has already completed with the given exit code.
    ///
    /// Used for builtins in pipelines: the builtin ran in-process, its output
    /// was written to a pipe, and this pseudo-child holds the read end.
    pub fn completed(exit_code: i32, pipes: HashMap<i32, File>) -> Self {
        ChildEx {
            inner: ChildInner::Completed(exit_code),
            pipes,
        }
    }

    /// Wait for the child to exit and return its exit code.
    pub fn wait(&mut self) -> io::Result<i32> {
        match &mut self.inner {
            ChildInner::Completed(code) => Ok(*code),
            #[cfg(unix)]
            ChildInner::Pid(pid) => {
                let mut status: nix::libc::c_int = 0;
                loop {
                    let ret = unsafe { nix::libc::waitpid(*pid, &mut status, 0) };
                    if ret == -1 {
                        let err = io::Error::last_os_error();
                        if err.kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        return Err(err);
                    }
                    break;
                }
                if nix::libc::WIFEXITED(status) {
                    Ok(nix::libc::WEXITSTATUS(status))
                } else if nix::libc::WIFSIGNALED(status) {
                    Ok(128 + nix::libc::WTERMSIG(status))
                } else {
                    Ok(128)
                }
            }
            #[cfg(windows)]
            ChildInner::Handle(handle) => {
                use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
                let wait_result = unsafe { WaitForSingleObject(*handle, INFINITE) };
                // WAIT_EVENT(0) is WAIT_OBJECT_0 — the object was signaled.
                if wait_result.0 != 0 {
                    let err = io::Error::last_os_error();
                    let _ = unsafe { windows::Win32::Foundation::CloseHandle(*handle) };
                    return Err(err);
                }
                let mut code: u32 = 0;
                unsafe { GetExitCodeProcess(*handle, &mut code) }
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                debug_assert!(
                    unsafe { windows::Win32::Foundation::CloseHandle(*handle) }.is_ok(),
                    "CloseHandle failed — possible double-close"
                );
                Ok(code as i32)
            }
        }
    }

    /// Take the read-end of a pipe for the given fd number.
    pub fn take_pipe(&mut self, fd: i32) -> Option<File> {
        self.pipes.remove(&fd)
    }
}

// Process replacement =============================================================

impl CommandEx {
    /// Replace the current process image with this command (Unix `execvp`).
    ///
    /// Applies FD redirections via `dup2`, changes CWD, sets environment,
    /// then calls `execvp`. On success, this function never returns.
    /// On failure, returns the OS error.
    #[cfg(unix)]
    pub fn exec_replace(self) -> io::Error {
        use std::ffi::{CStr, CString};
        use std::os::fd::IntoRawFd;
        use std::os::unix::ffi::OsStrExt;

        // Apply FD redirections via dup2.
        for (&fd_num, fd_spec) in &self.fds {
            match fd_spec {
                Fd::File(file) => {
                    if let Ok(cloned) = file.try_clone() {
                        let raw = cloned.into_raw_fd();
                        // SAFETY: raw is a valid fd from into_raw_fd; fd_num is from self.fds.
                        unsafe { nix::libc::dup2(raw, fd_num) };
                        // SAFETY: raw is valid and no longer needed (dup2 made a copy).
                        unsafe { nix::libc::close(raw) };
                    }
                }
                Fd::Pipe | Fd::InputPipe => {} // Not meaningful for exec replacement.
            }
        }

        // Change CWD (no RAII guard — we're replacing the process).
        if let Some(ref cwd) = self.cwd {
            let _ = std::env::set_current_dir(cwd);
        }

        // Build CString argv and envp.
        let argv_c: Vec<CString> = self
            .argv
            .iter()
            .map(|a| CString::new(a.as_bytes()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e)))
            .collect::<Result<_, _>>()
            .unwrap_or_default();

        let envp_c: Vec<CString> = self
            .env
            .iter()
            .map(|(k, v)| {
                let mut s = k.as_bytes().to_vec();
                s.push(b'=');
                s.extend_from_slice(v.as_bytes());
                CString::new(s).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
            })
            .collect::<Result<_, _>>()
            .unwrap_or_default();

        let path_c = match CString::new(self.path.as_bytes()) {
            Ok(c) => c,
            Err(e) => return io::Error::new(io::ErrorKind::InvalidInput, e),
        };

        // Resolve the executable path via PATH if it's a bare name.
        let resolved = if self.path.to_string_lossy().contains('/') {
            path_c
        } else {
            // Search PATH manually for execve (which doesn't do PATH lookup).
            let path_var = std::env::var("PATH").unwrap_or_default();
            let mut found = None;
            for dir in path_var.split(':') {
                let candidate = format!("{}/{}", dir, self.path.to_string_lossy());
                if let Ok(c) = CString::new(candidate.as_bytes()) {
                    if std::path::Path::new(&candidate).is_file() {
                        found = Some(c);
                        break;
                    }
                }
            }
            found.unwrap_or(path_c)
        };

        // execve replaces the process image. Only returns on error.
        let argv_refs: Vec<&CStr> = argv_c.iter().map(|c| c.as_c_str()).collect();
        let envp_refs: Vec<&CStr> = envp_c.iter().map(|c| c.as_c_str()).collect();
        let err = nix::unistd::execve(&resolved, &argv_refs, &envp_refs).unwrap_err();
        let kind = match err {
            nix::Error::ENOENT => io::ErrorKind::NotFound,
            nix::Error::EACCES => io::ErrorKind::PermissionDenied,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, err)
    }
}

// Command-line quoting ============================================================

/// POSIX shell quoting: single-quote each argument, escaping embedded `'`.
#[cfg(unix)]
#[allow(dead_code)] // Only used on Unix; tested via unit tests.
fn commandline_posix(argv: &[OsString]) -> OsString {
    use std::os::unix::ffi::OsStrExt;
    let mut result = Vec::<u8>::new();
    for (i, arg) in argv.iter().enumerate() {
        if i > 0 {
            result.push(b' ');
        }
        result.push(b'\'');
        for &byte in arg.as_bytes() {
            if byte == b'\'' {
                result.extend_from_slice(b"'\\''");
            } else {
                result.push(byte);
            }
        }
        result.push(b'\'');
    }
    OsStr::from_bytes(&result).to_os_string()
}

/// Windows MSVC CRT quoting.
///
/// Each argument is wrapped in double quotes. Backslashes before a double-quote
/// are doubled; a trailing run of backslashes (before the closing quote) is
/// also doubled. All other characters are literal.
///
/// Reference: <https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments>
#[cfg(windows)]
fn commandline_windows(argv: &[OsString]) -> OsString {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let mut out: Vec<u16> = Vec::new();
    for (i, arg) in argv.iter().enumerate() {
        if i > 0 {
            out.push(b' ' as u16);
        }
        let wide: Vec<u16> = arg.encode_wide().collect();
        out.push(b'"' as u16);
        let mut backslashes = 0usize;
        for &ch in &wide {
            if ch == b'\\' as u16 {
                backslashes += 1;
            } else if ch == b'"' as u16 {
                // Double backslashes before the quote, then escape the quote.
                for _ in 0..backslashes {
                    out.push(b'\\' as u16);
                }
                backslashes = 0;
                out.push(b'\\' as u16);
                out.push(b'"' as u16);
            } else {
                // Emit accumulated backslashes as-is (not before a quote).
                for _ in 0..backslashes {
                    out.push(b'\\' as u16);
                }
                backslashes = 0;
                out.push(ch);
            }
        }
        // Double trailing backslashes (they precede the closing quote).
        for _ in 0..backslashes {
            out.push(b'\\' as u16);
        }
        out.push(b'"' as u16);
    }
    OsString::from_wide(&out)
}

/// Verify `CommandLineToArgvW` round-trips our command line to the original argv.
#[cfg(windows)]
fn debug_assert_commandline_roundtrips(cmdline: &OsStr, expected_argv: &[OsString]) {
    if !cfg!(debug_assertions) {
        return;
    }
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::CommandLineToArgvW;

    let wide: Vec<u16> = cmdline.encode_wide().chain(std::iter::once(0)).collect();
    let mut argc: i32 = 0;
    let argv_ptr = unsafe { CommandLineToArgvW(PCWSTR(wide.as_ptr()), &mut argc) };
    if argv_ptr.is_null() {
        return;
    }
    let parsed: Vec<OsString> = (0..argc as usize)
        .map(|i| {
            let ptr = unsafe { *argv_ptr.add(i) };
            let len = unsafe { (0..65536).take_while(|&j| *ptr.0.add(j) != 0).count() };
            let slice = unsafe { std::slice::from_raw_parts(ptr.0, len) };
            OsString::from_wide(slice)
        })
        .collect();
    unsafe { windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(argv_ptr as *mut _))) };
    debug_assert_eq!(
        parsed, expected_argv,
        "commandline() round-trip failed!\n  cmdline: {:?}\n  expected: {:?}\n  got:      {:?}",
        cmdline, expected_argv, parsed,
    );
}

// Platform spawn ==================================================================

/// RAII guard that restores the process CWD on drop.
#[cfg(unix)]
struct CwdGuard {
    prev: std::path::PathBuf,
}

#[cfg(unix)]
impl CwdGuard {
    fn set(new_cwd: &std::path::Path) -> io::Result<Self> {
        let prev = std::env::current_dir()?;
        std::env::set_current_dir(new_cwd)?;
        Ok(Self { prev })
    }
}

#[cfg(unix)]
impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.prev);
    }
}

/// Set FD_CLOEXEC on a File so it isn't inherited by child processes.
///
/// On macOS, `pipe()` doesn't set O_CLOEXEC, so parent-side pipe ends
/// must be explicitly marked to prevent leaking into the child.
#[cfg(unix)]
fn set_cloexec(file: &File) {
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    unsafe {
        let flags = nix::libc::fcntl(fd, nix::libc::F_GETFD);
        if flags >= 0 {
            nix::libc::fcntl(fd, nix::libc::F_SETFD, flags | nix::libc::FD_CLOEXEC);
        }
    }
}

/// Close a list of raw file descriptors, ignoring errors.
#[cfg(unix)]
fn close_raw_fds(fds: &[std::os::fd::RawFd]) {
    for &fd in fds {
        unsafe { nix::libc::close(fd) };
    }
}

#[cfg(unix)]
fn spawn_impl(cmd: CommandEx) -> io::Result<ChildEx> {
    use nix::spawn::{posix_spawnp, PosixSpawnFileActions};
    use std::ffi::CString;
    use std::os::fd::IntoRawFd;
    use std::os::unix::ffi::OsStrExt;

    let mut file_actions = PosixSpawnFileActions::init().map_err(io::Error::other)?;

    let mut pipes: HashMap<i32, File> = HashMap::new();
    // Raw FDs that must be closed in the parent after spawn (or on error).
    let mut raw_fds_to_close: Vec<std::os::fd::RawFd> = Vec::new();

    for (&fd_num, fd_spec) in &cmd.fds {
        match fd_spec {
            Fd::Pipe => {
                let (read_end, write_end) = nix::unistd::pipe().map_err(io::Error::other)?;
                let write_raw = write_end.into_raw_fd();
                raw_fds_to_close.push(write_raw);
                file_actions.add_dup2(write_raw, fd_num).map_err(|e| {
                    close_raw_fds(&raw_fds_to_close);
                    io::Error::other(e)
                })?;
                // Parent reads from this pipe. Set CLOEXEC on the parent's
                // read-end so it isn't leaked to the child.
                let parent_file = File::from(read_end);
                set_cloexec(&parent_file);
                pipes.insert(fd_num, parent_file);
            }
            Fd::InputPipe => {
                let (read_end, write_end) = nix::unistd::pipe().map_err(io::Error::other)?;
                let read_raw = read_end.into_raw_fd();
                raw_fds_to_close.push(read_raw);
                file_actions.add_dup2(read_raw, fd_num).map_err(|e| {
                    close_raw_fds(&raw_fds_to_close);
                    io::Error::other(e)
                })?;
                // Parent writes to this pipe. Set CLOEXEC on the parent's
                // write-end so it isn't leaked to the child.
                let parent_file = File::from(write_end);
                set_cloexec(&parent_file);
                pipes.insert(fd_num, parent_file);
            }
            Fd::File(file) => {
                let raw_fd = file.try_clone()?.into_raw_fd();
                raw_fds_to_close.push(raw_fd);
                file_actions.add_dup2(raw_fd, fd_num).map_err(|e| {
                    close_raw_fds(&raw_fds_to_close);
                    io::Error::other(e)
                })?;
            }
        }
    }

    // Change directory before spawn if requested. We save and restore since
    // posix_spawn_file_actions_addchdir_np is not portable.
    // NOTE: set_current_dir is process-global, so concurrent threads would
    // observe the changed directory. This is safe because thaum is
    // single-threaded, but would need revisiting if concurrency is added.
    let _cwd_guard = if let Some(ref cwd) = cmd.cwd {
        Some(CwdGuard::set(cwd)?)
    } else {
        None
    };

    let argv_c: Vec<CString> = cmd
        .argv
        .iter()
        .map(|a| CString::new(a.as_bytes()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e)))
        .collect::<io::Result<_>>()?;

    let envp_c: Vec<CString> = cmd
        .env
        .iter()
        .map(|(k, v)| {
            let mut s = k.as_bytes().to_vec();
            s.push(b'=');
            s.extend_from_slice(v.as_bytes());
            CString::new(s).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
        })
        .collect::<io::Result<_>>()?;

    let path_c = CString::new(cmd.path.as_bytes()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let attrs = nix::spawn::PosixSpawnAttr::init().map_err(io::Error::other)?;

    let result = posix_spawnp(&path_c, &file_actions, &attrs, &argv_c, &envp_c).map_err(|e| {
        let kind = match e {
            nix::Error::ENOENT => io::ErrorKind::NotFound,
            nix::Error::EACCES => io::ErrorKind::PermissionDenied,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, e)
    });

    // Close raw FDs in the parent so readers get EOF.
    close_raw_fds(&raw_fds_to_close);

    // CWD is restored when _cwd_guard drops (here or on early return).
    drop(_cwd_guard);

    let pid = result?;
    Ok(ChildEx {
        inner: ChildInner::Pid(pid.as_raw()),
        pipes,
    })
}

#[cfg(windows)]
fn spawn_impl(cmd: CommandEx) -> io::Result<ChildEx> {
    spawn_windows::spawn_impl(cmd)
}

#[cfg(not(any(unix, windows)))]
fn spawn_impl(_cmd: CommandEx) -> io::Result<ChildEx> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "process spawning not supported on this platform",
    ))
}

#[cfg(windows)]
#[path = "command_ex/spawn_windows.rs"]
mod spawn_windows;

// Tests ===========================================================================

#[cfg(test)]
#[path = "command_ex_tests.rs"]
mod tests;
