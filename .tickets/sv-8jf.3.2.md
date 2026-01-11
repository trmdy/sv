---
id: sv-8jf.3.2
status: closed
deps: []
links: []
created: 2025-12-31T14:37:31.32987564+01:00
type: task
priority: 0
parent: sv-8jf.3
---
# sv take: create lease reservations

Implement sv take <pathspec...> [--strength <lvl>] [--intent <kind>] [--scope <scope>] [--ttl <dur>] [--note <text>] [--hint-lines ...] [--hint-symbol ...]:
- Parse and validate all flags
- Validate pathspec syntax (file/dir/glob)
- Generate lease UUID
- Compute expires_at from TTL
- Assign current actor as owner (or ownerless if no actor)
- Write lease to .git/sv/leases.jsonl

NOTE: Conflict detection uses the compatibility rules from sv-8jf.3.8.
NOTE: Note requirement validation is in sv-8jf.3.11.

Acceptance: sv take src/auth/** creates lease record with all fields


