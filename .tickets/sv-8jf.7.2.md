---
id: sv-8jf.7.2
status: closed
deps: []
links: []
created: 2025-12-31T14:39:03.937797677+01:00
type: task
priority: 1
parent: sv-8jf.7
---
# sv op log: display operation history

Implement sv op log:
- Show operations in reverse chronological order
- Display: op_id, timestamp, actor, command, affected refs/workspaces, outcome
- Support --limit N to restrict output
- Support --json output
- Filter by actor, time range, operation type

Acceptance: sv op log shows clear history of sv operations


