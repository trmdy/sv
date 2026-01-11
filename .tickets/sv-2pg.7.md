---
id: sv-2pg.7
status: closed
deps: [sv-2pg.2, sv-2pg.3]
links: []
created: 2025-12-31T15:24:03.76013554+01:00
type: task
priority: 1
parent: sv-2pg
---
# sv protect status UX

Implement a polished `sv protect status` UX:
- Human output: list rules with mode, show disabled patterns for this workspace, and highlight staged matches.
- JSON output: structured rules + match info.
- Integrates with protected paths config (sv-8jf.4.1).

Why: users need fast clarity on why commits may be blocked.


## Acceptance Criteria

sv protect status renders a clear table/list of rules, per-path mode overrides, workspace overrides, and staged matches; JSON output matches schema.


