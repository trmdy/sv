---
id: sv-8jf.3.7
status: closed
deps: []
links: []
created: 2025-12-31T14:37:45.749906482+01:00
type: task
priority: 2
parent: sv-8jf.3
---
# sv lease break: force-release with audit

Implement sv lease break <lease-id...> --reason <text>:
- Break-glass override to release any lease
- Requires --reason (mandatory explanation)
- Records break action in oplog with full audit trail
- Notifies (via output) the affected actor

Acceptance: sv lease break <id> --reason 'emergency fix' releases and audits


