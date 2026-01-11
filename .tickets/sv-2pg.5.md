---
id: sv-2pg.5
status: closed
deps: [sv-2pg.2, sv-2pg.3]
links: []
created: 2025-12-31T15:23:49.97024874+01:00
type: task
priority: 1
parent: sv-2pg
---
# sv init output polish

Improve `sv init` UX:
- Human output: clear summary of what changed and next recommended commands.
- JSON output: structured report per schema.
- Errors are surfaced with remediation (e.g., not a git repo, commondir issues).

Why: first‑run experience should feel polished and confidence‑building.


## Acceptance Criteria

sv init uses the shared output helper and prints a clear summary, next steps, and JSON output that matches schema; no ambiguous 'nothing to do' without context.


