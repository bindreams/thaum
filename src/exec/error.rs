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
