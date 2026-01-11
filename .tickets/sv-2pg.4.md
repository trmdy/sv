---
id: sv-2pg.4
status: closed
deps: [sv-2pg.1, sv-2pg.2]
links: []
created: 2025-12-31T15:23:42.630457317+01:00
type: task
priority: 1
parent: sv-2pg
---
# Global output flags + events plumbing

Add global output flags and event plumbing consistent with the output contract:
- `--events [path]` emits JSONL event stream to stdout or file.
- Define interaction with `--json` and `--quiet` (documented in sv-2pg.1).
- Wire event emission for at least `sv take` (lease_created) and `sv release` (lease_released).

Why: integration‑friendly UX without forcing external coordinators.


## Acceptance Criteria

CLI supports --events (stdout or file), --json behavior is consistent, and events are emitted as JSONL for relevant commands (at least sv take). Help text updated.


