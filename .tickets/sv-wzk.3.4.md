---
id: sv-wzk.3.4
status: closed
deps: []
links: []
created: 2025-12-31T14:45:14.080953957+01:00
type: task
priority: 2
parent: sv-wzk.3
---
# Hoist state storage and conflict records

Implement hoist state persistence in .git/sv/hoist/:
- Track last hoist operation per dest-ref
- Store conflict records when --continue-on-conflict used
- Track which commits were included/skipped
- Enable resumption after conflict resolution
- Clean up old hoist state on successful completion

Per spec Section 13.3

Acceptance: hoist state persists across invocations, conflicts tracked


