use std::path::PathBuf;

use serde_json::Value;
use sv::error::{exit_codes, Error, JsonError};

#[test]
fn exit_code_user_error() {
    let err = Error::InvalidArgument("bad input".to_string());
    assert_eq!(err.exit_code(), exit_codes::USER_ERROR);
}

#[test]
fn exit_code_policy_blocked() {
    let err = Error::ProtectedPath(PathBuf::from("Cargo.lock"));
    assert_eq!(err.exit_code(), exit_codes::POLICY_BLOCKED);
}

#[test]
fn exit_code_operation_failed() {
    let err = Error::OperationFailed("boom".to_string());
    assert_eq!(err.exit_code(), exit_codes::OPERATION_FAILED);
}

#[test]
fn details_include_lease_conflict_fields() {
    let err = Error::LeaseConflict {
        path: PathBuf::from("src/lib.rs"),
        holder: "alice".to_string(),
        strength: "exclusive".to_string(),
    };
    let details = err.details().expect("details");
    assert_eq!(details["path"], Value::String("src/lib.rs".to_string()));
    assert_eq!(details["holder"], Value::String("alice".to_string()));
    assert_eq!(details["strength"], Value::String("exclusive".to_string()));
}

#[test]
fn json_error_includes_details() {
    let err = Error::InvalidConfig("bad config".to_string());
    let json = JsonError::from(&err);
    assert_eq!(json.code, exit_codes::USER_ERROR);
    let details = json.details.expect("details");
    assert_eq!(details["message"], Value::String("bad config".to_string()));
}
