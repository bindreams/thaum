//! Subshell serialization payload. The parent serializes `Environment` + body
//! as JSON and pipes it to a child `thaum exec-ast` process.

use serde::{Deserialize, Serialize};

use crate::ast::Line;
use crate::exec::environment::SerializedEnvironment;

/// Payload sent to a child `thaum exec-ast` process for subshell execution.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubshellPayload {
    pub env: SerializedEnvironment,
    pub body: Vec<Line>,
    /// Shell options inherited from the parent executor.
    pub options: crate::dialect::ShellOptions,
    /// FD numbers from the parent's fd_table that were passed to the child
    /// via `CommandEx.fds`. The child reconstructs `fd_table` entries by
    /// duping these inherited OS file descriptors.
    #[serde(default)]
    pub inherited_fds: Vec<i32>,
}
