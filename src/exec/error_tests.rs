//! Unit tests for `ExecError` classification methods.

use super::error::ExecError;
use crate::test_labels::EXEC;

skuld::default_labels!(EXEC);

#[skuld::test]
fn control_flow_classification() {
    assert!(ExecError::ExitRequested(0).is_control_flow());
    assert!(ExecError::BreakRequested(1).is_control_flow());
    assert!(ExecError::ContinueRequested(1).is_control_flow());
    assert!(ExecError::ReturnRequested(0).is_control_flow());

    assert!(!ExecError::CommandNotFound("x".into()).is_control_flow());
    assert!(!ExecError::ReadonlyVariable("x".into()).is_control_flow());
    assert!(!ExecError::DivisionByZero.is_control_flow());
    assert!(!ExecError::UnboundVariable("x".into()).is_control_flow());
    assert!(!ExecError::BadSubstitution("x".into()).is_control_flow());
    assert!(!ExecError::BadRedirect("x".into()).is_control_flow());
    assert!(!ExecError::InvalidNumber("x".into(), "y".into()).is_control_flow());
    assert!(!ExecError::UnsupportedFeature("x".into()).is_control_flow());
}

#[skuld::test]
fn exit_status_mapping() {
    assert_eq!(ExecError::CommandNotFound("x".into()).exit_status(), 127);
    assert_eq!(ExecError::ReadonlyVariable("x".into()).exit_status(), 1);
    assert_eq!(ExecError::DivisionByZero.exit_status(), 1);
    assert_eq!(ExecError::UnboundVariable("x".into()).exit_status(), 1);
    assert_eq!(ExecError::BadSubstitution("x".into()).exit_status(), 1);
    assert_eq!(ExecError::BadRedirect("x".into()).exit_status(), 1);
    assert_eq!(ExecError::InvalidNumber("x".into(), "y".into()).exit_status(), 1);
    assert_eq!(ExecError::UnsupportedFeature("x".into()).exit_status(), 1);
    assert_eq!(ExecError::ExitRequested(42).exit_status(), 42);
    assert_eq!(ExecError::ReturnRequested(3).exit_status(), 3);
}

#[skuld::test]
fn io_permission_denied_exit_status_126() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    assert_eq!(ExecError::Io(io_err).exit_status(), 126);
}

#[skuld::test]
fn io_other_exit_status_1() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
    assert_eq!(ExecError::Io(io_err).exit_status(), 1);
}
