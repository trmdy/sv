---
id: sv-8jf.7.1
status: closed
deps: []
links: []
created: 2025-12-31T14:39:01.25477358+01:00
type: task
priority: 0
parent: sv-8jf.7
---
# Operation log storage format and writing

Implement operation log infrastructure:
- Storage in .git/sv/oplog/ as append-only files
- Schema: op_id, timestamp, actor, command, affected_refs, affected_workspaces, outcome, undo_data
- Atomic append (lock + write + unlock)
- Each major operation records its own log entry
- undo_data captures old/new ref tips, created/deleted paths, lease changes

Per spec Section 14

Acceptance: operations recorded atomically, can be read back


