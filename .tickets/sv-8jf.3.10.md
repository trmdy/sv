---
id: sv-8jf.3.10
status: closed
deps: []
links: []
created: 2025-12-31T14:45:08.661843856+01:00
type: task
priority: 2
parent: sv-8jf.3
---
# Ownerless lease support

Implement ownerless leases per spec Section 6.3:
- Allow sv take without actor set (creates ownerless lease)
- Ownerless leases act as 'shared warnings' / 'FYI hot zones'
- Anyone can release ownerless leases
- Show clearly in sv lease ls output (no actor column or 'shared')
- Compatibility: ownerless leases don't block others, just warn

Acceptance: sv take src/hot/** with no actor creates shared warning lease


