---
id: sv-wzk.3.1
status: closed
deps: []
links: []
created: 2025-12-31T14:39:57.651519594+01:00
type: task
priority: 1
parent: sv-wzk.3
---
# sv hoist: stack strategy implementation

Implement sv hoist command structure and integration branch:
- Parse -s selector, -d dest-ref, --strategy, --order flags
- Create/update integration branch: sv/hoist/<dest-ref>
- Reset integration branch to dest-ref
- Command infrastructure and validation

NOTE: Commit selection, deduplication, and replay are separate tasks.

Acceptance: sv hoist parses args, creates integration branch structure


