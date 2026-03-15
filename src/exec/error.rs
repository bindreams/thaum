//! Execution errors and control-flow signals (`ExitRequested`, `BreakRequested`,
//! `ReturnRequested`). Control-flow variants are not real errors -- they unwind
//! the call stack to the nearest loop or function boundary.

use std::io;
use thiserror::Error;

/// Errors that can occur during shell execution.
#[derive(Debug, Error)]
pub enum ExecError {
    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("{0}")]
    Io(#[from] io::Error),

    #[error("bad redirect: {0}")]
    BadRedirect(String),

    #[error("bad substitution: {0}")]
    BadSubstitution(String),

    #[error("division by zero")]
    DivisionByZero,

    #[error("readonly variable: {0}")]
    ReadonlyVariable(String),

    #[error("{0}: {1}")]
    InvalidNumber(String, String),

    #[error("{0}: unbound variable")]
    UnboundVariable(String),

    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),

    // Control flow signals (not real errors) --------------------------------------------------------------------------
    #[error("exit requested: {0}")]
    ExitRequested(i32),

    #[error("break requested: {0}")]
    BreakRequested(usize),

    #[error("continue requested: {0}")]
    ContinueRequested(usize),

    #[error("return requested: {0}")]
    ReturnRequested(i32),
}

impl ExecError {
    /// Whether this is a control-flow signal that must propagate to its boundary
    /// (loop for break/continue, function for return, top-level for exit).
    pub fn is_control_flow(&self) -> bool {
        matches!(
            self,
            Self::ExitRequested(_) | Self::BreakRequested(_) | Self::ContinueRequested(_) | Self::ReturnRequested(_)
        )
    }

    /// The exit status a real shell would set for this error.
    pub fn exit_status(&self) -> i32 {
        match self {
            Self::CommandNotFound(_) => 127,
            Self::Io(e) if e.kind() == io::ErrorKind::PermissionDenied => 126,
            Self::ExitRequested(code) | Self::ReturnRequested(code) => *code,
            _ => 1,
        }
    }
}
