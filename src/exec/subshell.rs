use serde::{Deserialize, Serialize};

use crate::ast::Line;
use crate::exec::environment::SerializedEnvironment;

/// Payload sent to a child `thaum exec-ast` process for subshell execution.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubshellPayload {
    pub env: SerializedEnvironment,
    pub body: Vec<Line>,
}
