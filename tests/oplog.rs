use chrono::{TimeZone, Utc};
use tempfile::TempDir;
use uuid::Uuid;

use sv::oplog::{format_record, OpLog, OpLogFilter, OpOutcome, OpRecord};

fn record_with(
    op_id: u128,
    timestamp: chrono::DateTime<Utc>,
    actor: Option<&str>,
    command: &str,
) -> OpRecord {
    let mut record = OpRecord::new(command, actor.map(|s| s.to_string()));
    record.op_id = Uuid::from_u128(op_id);
    record.timestamp = timestamp;
    record
}

#[test]
fn read_filtered_orders_and_limits() {
    let temp = TempDir::new().unwrap();
    let log = OpLog::new(temp.path().join("oplog"));

    let t1 = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2024, 1, 2, 12, 0, 0).unwrap();
    let t3 = Utc.with_ymd_and_hms(2024, 1, 3, 12, 0, 0).unwrap();

    log.append(&record_with(1, t1, Some("alpha"), "sv init"))
        .unwrap();
    log.append(&record_with(2, t2, Some("beta"), "sv op log"))
        .unwrap();
    log.append(&record_with(3, t3, Some("alpha"), "sv ws list"))
        .unwrap();

    let records = log.read_filtered(&OpLogFilter::default(), Some(2)).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].op_id, Uuid::from_u128(3));
    assert_eq!(records[1].op_id, Uuid::from_u128(2));
}

#[test]
fn read_filtered_by_actor_operation_and_time() {
    let temp = TempDir::new().unwrap();
    let log = OpLog::new(temp.path().join("oplog"));

    let t1 = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2024, 2, 2, 0, 0, 0).unwrap();

    log.append(&record_with(10, t1, Some("alpha"), "sv init"))
        .unwrap();
    log.append(&record_with(11, t2, Some("alpha"), "sv ws list"))
        .unwrap();

    let filter = OpLogFilter {
        actor: Some("alpha".to_string()),
        since: Some(t1),
        until: Some(t1),
        operation: Some("init".to_string()),
    };

    let records = log.read_filtered(&filter, None).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].command, "sv init");
}

#[test]
fn format_record_includes_core_fields() {
    let timestamp = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
    let mut record = record_with(99, timestamp, Some("actor1"), "sv init");
    record.outcome = OpOutcome::failed("oops");
    record.affected_refs = vec!["refs/heads/main".to_string()];
    record.affected_workspaces = vec!["ws1".to_string()];

    let formatted = format_record(&record);
    assert!(formatted.contains("actor=actor1"));
    assert!(formatted.contains("sv init"));
    assert!(formatted.contains("refs=[refs/heads/main]"));
    assert!(formatted.contains("workspaces=[ws1]"));
    assert!(formatted.contains("failed (oops)"));
}
