use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::path::Path;
use std::process::{Child, Stdio};

/// Extended process command builder with support for arbitrary FD mappings.
///
/// Wraps `std::process::Command` and adds `fd_mapping()` for passing file
/// descriptors 3+ to child processes. All platform-specific complexity
/// (Unix `pre_exec` + `dup2`, Windows CRT `_spawnvp` + `lpReserved2`)
/// is encapsulated inside `spawn()`.
///
/// For FDs 0-2, use the standard `stdin()`, `stdout()`, `stderr()` methods
/// which delegate to the inner `Command`. For FDs 3+, use `fd_mapping()`.
pub(crate) struct CommandEx {
    inner: std::process::Command,
    fd_mappings: Vec<(i32, File)>,
}

impl CommandEx {
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        CommandEx {
            inner: std::process::Command::new(program),
            fd_mappings: Vec::new(),
        }
    }

    /// Add a single argument. Currently unused but part of the CommandEx API
    /// (mirrors std::process::Command for FD 3+ support).
    #[allow(dead_code)]
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.inner.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(args);
        self
    }

    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.env(key, val);
        self
    }

    pub fn env_clear(&mut self) -> &mut Self {
        self.inner.env_clear();
        self
    }

    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.inner.current_dir(dir);
        self
    }

    pub fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }

    pub fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }

    pub fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }

    /// Register a file descriptor to pass to the child process.
    ///
    /// `child_fd` is the FD number the child will see (must be >= 3; use
    /// `stdin`/`stdout`/`stderr` for FDs 0-2). The `File` is kept alive
    /// until `spawn()` and is not consumed — the child gets an independent
    /// handle pointing to the same underlying OS resource.
    pub fn fd_mapping(&mut self, child_fd: i32, file: File) -> &mut Self {
        debug_assert!(child_fd >= 3, "use stdin/stdout/stderr for FDs 0-2");
        self.fd_mappings.push((child_fd, file));
        self
    }

    /// Spawn the child process, passing any registered FD mappings.
    pub fn spawn(&mut self) -> io::Result<Child> {
        self.spawn_impl()
    }
}

// --- Unix implementation ---

#[cfg(unix)]
impl CommandEx {
    fn spawn_impl(&mut self) -> io::Result<Child> {
        if !self.fd_mappings.is_empty() {
            use command_fds::{CommandFdExt, FdMapping};

            let mappings: Vec<FdMapping> = self
                .fd_mappings
                .drain(..)
                .map(|(child_fd, file)| FdMapping {
                    parent_fd: file.into(),
                    child_fd,
                })
                .collect();
            self.inner.fd_mappings(mappings).map_err(io::Error::other)?;
        }
        self.inner.spawn()
    }
}

// --- Windows implementation ---

#[cfg(windows)]
impl CommandEx {
    fn spawn_impl(&mut self) -> io::Result<Child> {
        if self.fd_mappings.is_empty() {
            return self.inner.spawn();
        }

        // TODO: Use winspawn to pass FDs 3+ via CRT _wspawnvp + lpReserved2.
        //
        // For now, spawn normally — the extra FDs are opened as a side effect
        // but not inherited by the child. This matches the previous behavior
        // and avoids blocking Windows compilation.
        //
        // When implemented, this will:
        // 1. Convert each File to winspawn::FileDescriptor via _open_osfhandle
        // 2. Use winspawn::move_fd() to place each at the target FD number
        // 3. Spawn via winspawn::spawn() (passes CRT FD table via lpReserved2)
        self.inner.spawn()
    }
}

// --- Fallback for other platforms ---

#[cfg(not(any(unix, windows)))]
impl CommandEx {
    fn spawn_impl(&mut self) -> io::Result<Child> {
        // FD mappings are not supported on this platform.
        self.inner.spawn()
    }
}
