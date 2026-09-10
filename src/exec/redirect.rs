//! Redirect resolution: opens files, sets up FD overrides, and applies them
//! to an `IoContext` for the duration of a single command.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};

use crate::ast::{Redirect, RedirectKind};
use crate::exec::buffered_file::{dup_process_fd, BufferedFile};
use crate::exec::error::ExecError;
use crate::exec::expand;
use crate::exec::io_context::IoContext;
use crate::exec::Executor;

/// Temporary redirect state for a single command.
///
/// Holds file handles opened by the command's redirect list. FDs 0-2 override
/// the IoContext; FDs 3+ are stored in `extra_fds` for dup resolution and
/// child process inheritance.
pub(super) struct ActiveRedirects {
    pub stdin: Option<BufferedFile>,
    pub stdout: Option<BufferedFile>,
    pub stderr: Option<BufferedFile>,
    pub extra_fds: HashMap<i32, BufferedFile>,
    /// FDs explicitly closed via `N>&-` / `N<&-`. Used by `exec` redirect-only
    /// mode to remove persistent FDs from the IoContext.
    pub closed_fds: HashSet<i32>,
}

impl ActiveRedirects {
    pub fn new() -> Self {
        ActiveRedirects {
            stdin: None,
            stdout: None,
            stderr: None,
            extra_fds: HashMap::new(),
            closed_fds: HashSet::new(),
        }
    }

    /// Returns true if any redirections are active.
    #[allow(dead_code)] // Kept for diagnostics and future use.
    pub fn is_active(&self) -> bool {
        self.stdin.is_some() || self.stdout.is_some() || self.stderr.is_some() || !self.extra_fds.is_empty()
    }

    /// Apply redirects to IoContext via save/restore. Returns saved fds for
    /// restoration when the command completes.
    ///
    /// `into_inner()` on BufferedFile is safe here: the buffer is always empty
    /// because BufferedFile was just created by `resolve_redirects` (no reads yet).
    pub fn apply(&mut self, io: &mut IoContext) -> SavedFds {
        let mut saved = SavedFds::new();
        if let Some(f) = self.stdin.take() {
            saved.push(0, io.is_closed(0), io.save_and_set(0, f.into_inner()));
        }
        if let Some(f) = self.stdout.take() {
            saved.push(1, io.is_closed(1), io.save_and_set(1, f.into_inner()));
        }
        if let Some(f) = self.stderr.take() {
            saved.push(2, io.is_closed(2), io.save_and_set(2, f.into_inner()));
        }
        for (fd, file) in self.extra_fds.drain() {
            saved.push(fd, io.is_closed(fd), io.save_and_set(fd, file.into_inner()));
        }
        for &fd in &self.closed_fds {
            let was_closed = io.is_closed(fd);
            if fd <= 2 {
                // Replace fds 0-2 with /dev/null instead of removing, so
                // downstream code always finds a valid handle for these fds,
                // then mark the number closed so children are still denied it.
                // The shell's own writes therefore succeed where bash reports
                // EBADF — that half is issue #41.
                let null = crate::exec::io_context::open_null_device();
                let prev = io.save_and_set(fd, null);
                io.set_closed_marker(fd, true);
                saved.push(fd, was_closed, prev);
            } else {
                let prev = io.remove_fd(fd);
                io.close_fd(fd);
                saved.push(fd, was_closed, prev);
            }
        }
        // Clear tty overrides for all touched fds so that redirected fds
        // don't falsely report as terminals.
        let touched: Vec<i32> = saved.fds.iter().map(|e| e.fd).collect();
        for fd in touched {
            if io.has_tty_override(fd) {
                saved.saved_tty_overrides.insert(fd);
                io.clear_tty_override(fd);
            }
        }
        saved
    }
}

/// Saved fd entries for restore-on-completion. Restores in reverse order
/// to correctly unwind chained dup redirects (e.g. `exec 3>&1 1>/dev/null`).
pub(super) struct SavedFds {
    fds: Vec<SavedFd>,
    saved_tty_overrides: HashSet<i32>,
}

/// One descriptor's pre-redirect state: its handle, and whether it was already
/// marked closed. Both must come back, or a per-command `N>&-` would leak its
/// close into the rest of the script.
struct SavedFd {
    fd: i32,
    was_closed: bool,
    file: Option<File>,
}

impl SavedFds {
    fn new() -> Self {
        SavedFds {
            fds: Vec::new(),
            saved_tty_overrides: HashSet::new(),
        }
    }

    fn push(&mut self, fd: i32, was_closed: bool, file: Option<File>) {
        self.fds.push(SavedFd { fd, was_closed, file });
    }

    /// Restore all saved fds back to the IoContext, in reverse order.
    /// Also restores tty overrides that were cleared during apply.
    pub fn restore(self, io: &mut IoContext) {
        for entry in self.fds.into_iter().rev() {
            io.restore(entry.fd, entry.file);
            io.set_closed_marker(entry.fd, entry.was_closed);
        }
        for fd in self.saved_tty_overrides {
            io.set_tty_override(fd);
        }
    }
}

impl Executor {
    /// Process a command's redirect list into an `ActiveRedirects`.
    ///
    /// Redirects are processed left-to-right. `>&N` resolves against FDs
    /// already opened in this redirect list, then against the IoContext.
    pub(super) fn resolve_redirects(
        &mut self,
        redirects: &[Redirect],
        io: &IoContext,
    ) -> Result<ActiveRedirects, ExecError> {
        let mut active = ActiveRedirects::new();

        for redirect in redirects {
            let fd = redirect.fd;
            match &redirect.kind {
                RedirectKind::Input(word) => {
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = File::open(&resolved).map_err(|e| ExecError::BadRedirect(format!("{path}: {e}")))?;
                    assign_read_fd(&mut active, fd.unwrap_or(0), file)?;
                }
                RedirectKind::Output(word) | RedirectKind::Clobber(word) => {
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = File::create(&resolved).map_err(|e| ExecError::BadRedirect(format!("{path}: {e}")))?;
                    assign_write_fd(&mut active, fd.unwrap_or(1), file)?;
                }
                RedirectKind::Append(word) => {
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&resolved)
                        .map_err(|e| ExecError::BadRedirect(format!("{path}: {e}")))?;
                    assign_write_fd(&mut active, fd.unwrap_or(1), file)?;
                }
                RedirectKind::ReadWrite(word) => {
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(&resolved)
                        .map_err(|e| ExecError::BadRedirect(format!("{path}: {e}")))?;
                    assign_read_fd(&mut active, fd.unwrap_or(0), file)?;
                }
                RedirectKind::DupOutput(word) => {
                    let target = expand::expand_word(word, &mut self.env)?;
                    let dest_fd = fd.unwrap_or(1);
                    if target == "-" {
                        // Close the FD: use sink for 0-2, remove for 3+.
                        close_write_fd(&mut active, dest_fd);
                    } else if let Some(src_fd) = parse_move_target(&target) {
                        // `N>&M-` — move: duplicate M onto N, then close M.
                        let cloned = clone_fd_for_write(&active, io, src_fd)?;
                        assign_write_fd(&mut active, dest_fd, cloned)?;
                        if src_fd != dest_fd {
                            close_write_fd(&mut active, src_fd);
                        }
                    } else if let Ok(src_fd) = target.parse::<i32>() {
                        let cloned = clone_fd_for_write(&active, io, src_fd)?;
                        // Cloned FDs share OS position — don't buffer reads.
                        assign_write_fd(&mut active, dest_fd, cloned)?;
                    } else {
                        return Err(ExecError::BadRedirect(format!("{target}: ambiguous redirect")));
                    }
                }
                RedirectKind::DupInput(word) => {
                    let target = expand::expand_word(word, &mut self.env)?;
                    let dest_fd = fd.unwrap_or(0);
                    if target == "-" {
                        close_read_fd(&mut active, dest_fd);
                    } else if let Some(src_fd) = parse_move_target(&target) {
                        // `N<&M-` — move: duplicate M onto N, then close M.
                        let cloned = clone_fd_for_read(&active, io, src_fd)?;
                        assign_read_bf(&mut active, dest_fd, BufferedFile::passthrough(cloned))?;
                        if src_fd != dest_fd {
                            close_read_fd(&mut active, src_fd);
                        }
                    } else if let Ok(src_fd) = target.parse::<i32>() {
                        let cloned = clone_fd_for_read(&active, io, src_fd)?;
                        // Cloned FDs share OS position — don't buffer reads.
                        assign_read_bf(&mut active, dest_fd, BufferedFile::passthrough(cloned))?;
                    } else {
                        return Err(ExecError::BadRedirect(format!("{target}: ambiguous redirect")));
                    }
                }
                RedirectKind::HereDoc { body, .. } => {
                    // Create a temporary file with the heredoc body, use as stdin.
                    let mut tmpfile = tempfile()?;
                    tmpfile.write_all(body.as_bytes()).map_err(ExecError::Io)?;
                    tmpfile.seek_to_start()?;
                    assign_read_fd(&mut active, fd.unwrap_or(0), tmpfile)?;
                }
                RedirectKind::BashHereString(word) => {
                    let expanded = expand::expand_word(word, &mut self.env)?;
                    let mut tmpfile = tempfile()?;
                    tmpfile.write_all(expanded.as_bytes()).map_err(ExecError::Io)?;
                    tmpfile.write_all(b"\n").map_err(ExecError::Io)?;
                    tmpfile.seek_to_start()?;
                    assign_read_fd(&mut active, fd.unwrap_or(0), tmpfile)?;
                }
                RedirectKind::BashOutputAll(word) => {
                    // &> file — redirect both stdout and stderr to file
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = File::create(&resolved).map_err(|e| ExecError::BadRedirect(format!("{path}: {e}")))?;
                    let clone = file.try_clone().map_err(ExecError::Io)?;
                    assign_write_fd(&mut active, 1, file)?;
                    assign_write_fd(&mut active, 2, clone)?;
                }
                RedirectKind::BashAppendAll(word) => {
                    // &>> file — append both stdout and stderr to file
                    let path = expand::expand_word(word, &mut self.env)?;
                    let resolved = self.resolve_path(&path);
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&resolved)
                        .map_err(|e| ExecError::BadRedirect(format!("{path}: {e}")))?;
                    let clone = file.try_clone().map_err(ExecError::Io)?;
                    assign_write_fd(&mut active, 1, file)?;
                    assign_write_fd(&mut active, 2, clone)?;
                }
            }
        }

        Ok(active)
    }
}

/// Recognise the move form of a dup target: `N-` means "duplicate N here, then
/// close N". A bare `-` is the plain close form and is handled by the caller.
fn parse_move_target(target: &str) -> Option<i32> {
    let digits = target.strip_suffix('-')?;
    // Digits only. `str::parse` would accept a leading `-`, so `3>&-3-` would
    // yield -3 and carry a negative descriptor into the closed set and on into
    // `dup2`. bash rejects the word instead.
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<i32>().ok()
}

/// Assign a file to the appropriate read FD slot (buffered).
fn assign_read_fd(active: &mut ActiveRedirects, fd: i32, file: File) -> Result<(), ExecError> {
    assign_read_bf(active, fd, BufferedFile::new(file))
}

/// Assign a pre-wrapped BufferedFile to the appropriate read FD slot.
fn assign_read_bf(active: &mut ActiveRedirects, fd: i32, bf: BufferedFile) -> Result<(), ExecError> {
    // Redirects apply left to right and the last word about a descriptor wins,
    // so reopening a number cancels an earlier close in the same list.
    active.closed_fds.remove(&fd);
    match fd {
        0 => active.stdin = Some(bf),
        n => {
            active.extra_fds.insert(n, bf);
        }
    }
    Ok(())
}

/// Assign a file to the appropriate write FD slot (passthrough — no read buffering).
fn assign_write_fd(active: &mut ActiveRedirects, fd: i32, file: File) -> Result<(), ExecError> {
    // Last one wins — see `assign_read_bf`.
    active.closed_fds.remove(&fd);
    match fd {
        1 => active.stdout = Some(BufferedFile::passthrough(file)),
        2 => active.stderr = Some(BufferedFile::passthrough(file)),
        n => {
            active.extra_fds.insert(n, BufferedFile::passthrough(file));
        }
    }
    Ok(())
}

/// Close a write FD (`N>&-`).
fn close_write_fd(active: &mut ActiveRedirects, fd: i32) {
    clear_fd_slot(active, fd);
}

/// Close a read FD (`N<&-`).
fn close_read_fd(active: &mut ActiveRedirects, fd: i32) {
    clear_fd_slot(active, fd);
}

/// Drop whatever this redirect list has assigned to `fd` and mark it closed.
///
/// The slot is chosen by descriptor *number*, not by the direction of the
/// closing operator: `0>&-` and `0<&-` both close descriptor 0. Keying off the
/// operator instead left `0>&-` routing to `extra_fds` while stdin stayed
/// assigned, so `exec 0<src 0>&-; read x` still read the file (bash: EBADF).
///
/// For FDs 0-2 the slot is only cleared here; `apply()` and `adopt_redirects`
/// substitute /dev/null so downstream code always finds a valid handle.
fn clear_fd_slot(active: &mut ActiveRedirects, fd: i32) {
    active.closed_fds.insert(fd);
    match fd {
        0 => active.stdin = None,
        1 => active.stdout = None,
        2 => active.stderr = None,
        n => {
            active.extra_fds.remove(&n);
        }
    }
}

/// Clone a file descriptor from the active redirects or IoContext
/// for use as a write target.
fn clone_fd_for_write(active: &ActiveRedirects, io: &IoContext, src_fd: i32) -> Result<File, ExecError> {
    // A descriptor the script closed is gone, even though the host process may
    // still have that number open. Without this check the `dup_process_fd`
    // fallback below reaches past the close into the embedder's fd table —
    // which is how `exec 3>&-` ended up writing to the host's descriptor 3.
    if active.closed_fds.contains(&src_fd) || io.is_closed(src_fd) {
        return Err(ExecError::BadRedirect(format!("{src_fd}: bad file descriptor")));
    }

    // Check active redirects first (FDs opened earlier in this redirect list).
    if let Some(file) = active.stdout.as_ref().filter(|_| src_fd == 1) {
        return file.try_clone().map_err(ExecError::Io);
    }
    if let Some(file) = active.stderr.as_ref().filter(|_| src_fd == 2) {
        return file.try_clone().map_err(ExecError::Io);
    }
    if let Some(file) = active.stdin.as_ref().filter(|_| src_fd == 0) {
        return file.try_clone().map_err(ExecError::Io);
    }
    if let Some(file) = active.extra_fds.get(&src_fd) {
        return file.try_clone().map_err(ExecError::Io);
    }
    // Check IoContext (contains both persistent fds and original process fds).
    if let Ok(file) = io.try_clone_fd(src_fd) {
        return Ok(file);
    }
    // Fall back to duplicating the process's own file descriptors (for fds
    // not in IoContext, e.g. inherited from parent).
    if let Some(file) = dup_process_fd(src_fd) {
        return Ok(file);
    }
    Err(ExecError::BadRedirect(format!("{src_fd}: bad file descriptor")))
}

/// Clone a file descriptor for use as a read source.
fn clone_fd_for_read(active: &ActiveRedirects, io: &IoContext, src_fd: i32) -> Result<File, ExecError> {
    // Same resolution order as clone_fd_for_write.
    clone_fd_for_write(active, io, src_fd)
}

/// Create a temporary file for heredoc/herestring content.
fn tempfile() -> Result<File, ExecError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("thaum-heredoc-{}-{}", std::process::id(), n,));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(ExecError::Io)?;

    // Best-effort removal of temp file. On Unix, the file remains accessible
    // via the open handle even after unlink. On Windows, this may fail (file
    // is still open), which is fine — the OS cleans up temp files.
    let _ = std::fs::remove_file(&path);

    Ok(file)
}

/// Extension trait to seek a File back to the start.
trait SeekToStart {
    fn seek_to_start(&mut self) -> Result<(), ExecError>;
}

impl SeekToStart for File {
    fn seek_to_start(&mut self) -> Result<(), ExecError> {
        use std::io::Seek;
        self.seek(io::SeekFrom::Start(0)).map_err(ExecError::Io)?;
        Ok(())
    }
}
