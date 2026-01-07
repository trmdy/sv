---
id: sv-2pg.6
status: closed
deps: [sv-2pg.2, sv-2pg.3]
links: []
created: 2025-12-31T15:23:57.232968937+01:00
type: task
priority: 1
parent: sv-2pg
---
# sv take output polish + actionable conflicts

Improve `sv take` UX:
- Human output: created leases, conflict summary, and suggested commands (e.g., `sv lease who <path>`, `--allow-overlap`).
- JSON output: include leases, conflicts, and summary counts.
- Ensure exit behavior is consistent with conflict rules.

Why: leasing is the primary coordination action; UX should reduce friction.


## Acceptance Criteria

sv take uses shared output helper; conflicts are summarized with clear next-step suggestions; JSON includes conflicts + created leases; exit codes match spec when conflicts occur.


