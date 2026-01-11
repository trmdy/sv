---
id: sv-8jf.3.3
status: closed
deps: []
links: []
created: 2025-12-31T14:37:34.297435394+01:00
type: task
priority: 1
parent: sv-8jf.3
---
# sv release: release lease reservations

Implement sv release <lease-id...> | sv release <pathspec...>:
- Release by explicit lease ID
- Or release by pathspec match (releases all matching leases owned by current actor)
- Mark lease as released (don't delete for audit trail)
- Validate actor ownership (can't release others' leases without --force)

Acceptance: sv release src/auth/** releases matching leases


