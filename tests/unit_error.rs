use std::path::PathBuf;

use sv::error::{exit_codes, Error, JsonError};

#[test]
fn exit_codes_map_correctly() {
    let user = Error::InvalidArgument("bad".to_string());
    assert_eq!(user.exit_code(), exit_codes::USER_ERROR);

    let policy = Error::ProtectedPath(PathBuf::from(".beads/issues.jsonl"));
    assert_eq!(policy.exit_code(), exit_codes::POLICY_BLOCKED);

    let op = Error::OperationFailed("boom".to_string());
    assert_eq!(op.exit_code(), exit_codes::OPERATION_FAILED);
}

#[test]
fn json_error_includes_code() {
    let err = Error::WorkspaceNotFound("agent1".to_string());
    let json = JsonError::from(&err);
    assert_eq!(json.code, exit_codes::USER_ERROR);
    assert!(json.error.contains("Workspace not found"));
}
