//! Dynamic variable dispatch: `get_dynamic`, `set_dynamic`, `unset_dynamic`.
//!
//! Dynamic variables are shell variables whose value is computed on read
//! (e.g., `RANDOM`, `SECONDS`, `BASHPID`) rather than stored statically.
//! The `special_active` set tracks which dynamic variables still have their
//! special behavior (unset kills it for Category A variables).

use std::time::{SystemTime, UNIX_EPOCH};

use super::Environment;
use crate::exec::error::ExecError;

/// Current Unix epoch time in seconds.
pub(super) fn epoch_secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Environment {
    /// Get a dynamic variable's computed value.
    ///
    /// Returns `Some(value)` for variables with active special behavior,
    /// `None` to fall through to regular variable lookup.
    pub fn get_dynamic(&mut self, name: &str) -> Option<String> {
        match name {
            "RANDOM" if self.special_active.contains("RANDOM") => {
                // POSIX/glibc LCG: next = (a * state + c), extract bits 16..30.
                self.random_state = self.random_state.wrapping_mul(1103515245).wrapping_add(12345);
                Some(((self.random_state >> 16) & 0x7fff).to_string())
            }
            "SECONDS" if self.special_active.contains("SECONDS") => {
                let elapsed = epoch_secs_now().saturating_sub(self.start_epoch_secs) as i64;
                Some((elapsed + self.seconds_offset).to_string())
            }
            "EPOCHSECONDS" if self.special_active.contains("EPOCHSECONDS") => Some(epoch_secs_now().to_string()),
            "EPOCHREALTIME" if self.special_active.contains("EPOCHREALTIME") => {
                let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
                Some(format!("{}.{:06}", dur.as_secs(), dur.subsec_micros()))
            }
            "SRANDOM" if self.special_active.contains("SRANDOM") => {
                let mut buf = [0u8; 4];
                getrandom::fill(&mut buf).unwrap_or_default();
                Some(u32::from_ne_bytes(buf).to_string())
            }
            "BASHPID" if self.special_active.contains("BASHPID") => Some(std::process::id().to_string()),
            "LINENO" if self.special_active.contains("LINENO") => {
                Some(((self.lineno as isize + self.lineno_offset) as usize).to_string())
            }
            "SHELLOPTS" => {
                let mut opts = Vec::new();
                if self.errexit {
                    opts.push("errexit");
                }
                // hashall is always on
                opts.push("hashall");
                opts.push("interactive-comments");
                if self.nounset {
                    opts.push("nounset");
                }
                if self.xtrace {
                    opts.push("xtrace");
                }
                Some(opts.join(":"))
            }
            "BASHOPTS" => {
                let mut opts = Vec::new();
                if self.expand_aliases {
                    opts.push("expand_aliases");
                }
                Some(opts.join(":"))
            }
            _ => None,
        }
    }

    /// Intercept writes to dynamic variables.
    ///
    /// Returns `Some(Ok(()))` if the write was handled (caller should not store),
    /// `Some(Err(_))` if the write is forbidden, or `None` to fall through.
    pub fn set_dynamic(&mut self, name: &str, value: &str) -> Option<Result<(), ExecError>> {
        match name {
            "RANDOM" if self.special_active.contains("RANDOM") => {
                // Assignment seeds the RNG.
                self.random_state = value.parse::<u32>().unwrap_or(0);
                Some(Ok(()))
            }
            "SECONDS" if self.special_active.contains("SECONDS") => {
                // Assignment resets the timer.
                let assigned: i64 = value.parse().unwrap_or(0);
                let elapsed = epoch_secs_now().saturating_sub(self.start_epoch_secs) as i64;
                self.seconds_offset = assigned - elapsed;
                Some(Ok(()))
            }
            "EPOCHSECONDS" if self.special_active.contains("EPOCHSECONDS") => {
                // Assignment accepted but overridden on next read.
                Some(Ok(()))
            }
            "EPOCHREALTIME" if self.special_active.contains("EPOCHREALTIME") => Some(Ok(())),
            "SRANDOM" if self.special_active.contains("SRANDOM") => {
                // Assignment silently ignored.
                Some(Ok(()))
            }
            "BASHPID" if self.special_active.contains("BASHPID") => {
                // Assignment silently ignored.
                Some(Ok(()))
            }
            "GROUPS" if self.special_active.contains("GROUPS") => {
                // Assignment silently ignored (Category D).
                Some(Ok(()))
            }
            // Call stack variables: assign silently ignored (Category C2/D).
            "FUNCNAME" | "BASH_SOURCE" | "BASH_LINENO" if self.in_function_scope() => Some(Ok(())),
            "LINENO" if self.special_active.contains("LINENO") => {
                // Assignment offsets subsequent line numbers.
                let assigned: isize = value.parse().unwrap_or(0);
                self.lineno_offset = assigned - self.lineno as isize;
                Some(Ok(()))
            }
            // Category C: readonly, cannot assign.
            "SHELLOPTS" | "BASHOPTS" => Some(Err(ExecError::ReadonlyVariable(name.to_string()))),
            // When OPTIND is set to 1, reset the getopts sub-index.
            "OPTIND" => {
                if value == "1" {
                    self.getopts_subindex = 0;
                }
                None // fall through to normal set_var
            }
            _ => None,
        }
    }

    /// Intercept unset of dynamic variables.
    ///
    /// Returns `Some(Ok(()))` if handled (Category A: kills special behavior),
    /// `Some(Err(_))` if unset is forbidden, or `None` to fall through.
    pub fn unset_dynamic(&mut self, name: &str) -> Option<Result<(), ExecError>> {
        match name {
            // Category C: cannot unset.
            "SHELLOPTS" | "BASHOPTS" => Some(Err(ExecError::ReadonlyVariable(name.to_string()))),
            // Category C2: cannot unset.
            "BASH_SOURCE" | "BASH_LINENO" => Some(Err(ExecError::ReadonlyVariable(name.to_string()))),
            // Category A: unset kills special behavior forever.
            "RANDOM" | "SECONDS" | "EPOCHSECONDS" | "EPOCHREALTIME" | "SRANDOM" | "LINENO" => {
                self.special_active.remove(name);
                // Also remove from the regular variable store if present.
                self.variables.remove(name);
                Some(Ok(()))
            }
            // Category D: unset works, removes variable.
            "BASHPID" => {
                self.special_active.remove("BASHPID");
                Some(Ok(()))
            }
            "GROUPS" if self.special_active.contains("GROUPS") => {
                self.special_active.remove("GROUPS");
                self.variables.remove("GROUPS");
                Some(Ok(()))
            }
            _ => None,
        }
    }

    /// Current line number (1-based, as set by `execute_lines`).
    pub fn lineno(&self) -> usize {
        self.lineno
    }

    /// Update the current line number (called by the executor).
    pub fn set_lineno(&mut self, lineno: usize) {
        self.lineno = lineno;
    }

    /// Get the getopts sub-index (position within a grouped option string like `-abc`).
    pub fn getopts_subindex(&self) -> usize {
        self.getopts_subindex
    }

    /// Set the getopts sub-index.
    pub fn set_getopts_subindex(&mut self, idx: usize) {
        self.getopts_subindex = idx;
    }
}
