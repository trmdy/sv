---
id: sv-8jf.3.4
status: closed
deps: []
links: []
created: 2025-12-31T14:37:36.654393586+01:00
type: task
priority: 1
parent: sv-8jf.3
---
# sv lease ls: list active leases

Implement sv lease ls [-s <selector>]:
- List all active (non-expired, non-released) leases
- Show: id, pathspec, strength, intent, actor, scope, expires_at, note
- Support --json output
- Selector filtering (stub for now)
- Filter by actor with --actor flag

Acceptance: sv lease ls shows all active leases with details


