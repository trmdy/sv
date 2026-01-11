---
id: sv-8jf.3.6
status: closed
deps: []
links: []
created: 2025-12-31T14:37:42.592915855+01:00
type: task
priority: 1
parent: sv-8jf.3
---
# sv lease renew: extend lease TTL

Implement sv lease renew <lease-id...> [--ttl <dur>]:
- Extend expires_at for specified leases
- Validate actor ownership
- Default extension: original TTL or config default
- Can renew multiple leases at once

Acceptance: sv lease renew <id> --ttl 4h extends expiry


