---
id: sv-8jf.5.3
status: closed
deps: []
links: []
created: 2025-12-31T14:38:33.342668818+01:00
type: task
priority: 0
parent: sv-8jf.5
---
# Lease conflict checking during commit

Implement lease conflict checking at commit time:
- Get list of files being committed
- Check for active exclusive/strong leases owned by OTHER actors
- Block commit (exit 3) if conflict found
- Optionally warn if committed paths were never leased (provenance warning; default warn-only)
- Support --force-lease to override

Per spec Section 6.4 - critical requirement

Acceptance: sv commit blocked when touching file under others' exclusive lease


