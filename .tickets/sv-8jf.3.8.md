---
id: sv-8jf.3.8
status: closed
deps: []
links: []
created: 2025-12-31T14:37:50.114993866+01:00
type: task
priority: 0
parent: sv-8jf.3
---
# Lease compatibility rules and conflict detection

Implement lease strength compatibility matrix:
- observe overlaps with anything
- cooperative overlaps with observe and cooperative
- strong overlaps with observe; cooperative only with --allow-overlap; blocks strong/exclusive
- exclusive blocks any overlapping lease except observe (configurable)

Include:
- Pathspec overlap detection (glob matching)
- Conflict checking on sv take
- Policy overrides from .sv.toml

Per spec Section 6.2

Acceptance: conflicts detected correctly, policy overrides work


