mod support;

use serde_json::Value;

use support::{sv_cmd, TestRepo};

#[test]
fn commit_writes_oplog_entry() -> Result<(), Box<dyn std::error::Error>> {
    let repo = TestRepo::init()?;
    repo.init_sv_dirs()?;
    repo.write_file("README.md", "# sv\n")?;
    repo.stage_path("README.md")?;

    sv_cmd()
        .current_dir(repo.path())
        .arg("commit")
        .arg("-m")
        .arg("test commit")
        .assert()
        .success();

    let output = sv_cmd()
        .current_dir(repo.path())
        .arg("op")
        .arg("log")
        .arg("--json")
        .output()?;
    assert!(output.status.success());

    let report: Value = serde_json::from_slice(&output.stdout)?;
    let records = report
        .get("records")
        .and_then(|value| value.as_array())
        .expect("records array");
    let commit_record = records
        .iter()
        .find(|record| {
            record
                .get("command")
                .and_then(|value| value.as_str())
                .map(|command| command.contains("commit"))
                .unwrap_or(false)
        })
        .expect("commit record");
    let details = commit_record
        .get("details")
        .and_then(|value| value.get("commit"))
        .expect("commit details");
    let commit_hash = details
        .get("commit_hash")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(!commit_hash.is_empty());
    let change_id = details
        .get("change_id")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(!change_id.is_empty());
    let files = details
        .get("files")
        .and_then(|value| value.as_array())
        .expect("files array");
    assert!(files
        .iter()
        .any(|value| value.as_str() == Some("README.md")));

    Ok(())
}
