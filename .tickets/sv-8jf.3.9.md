---
id: sv-8jf.3.9
status: closed
deps: []
links: []
created: 2025-12-31T14:37:52.645022292+01:00
type: task
priority: 1
parent: sv-8jf.3
---
# Lease TTL expiration and cleanup

Implement lease expiration:
- Check expires_at on all lease operations
- Expired leases treated as inactive
- Periodic cleanup (on any lease command) to archive expired leases
- Grace period option (configurable)

Acceptance: expired leases don't block, cleanup maintains performance


