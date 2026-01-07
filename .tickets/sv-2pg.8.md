---
id: sv-2pg.8
status: closed
deps: [sv-2pg.2, sv-2pg.3]
links: []
created: 2025-12-31T15:24:10.648842683+01:00
type: task
priority: 1
parent: sv-2pg
---
# sv status UX (single-pane view)

Implement `sv status` as the CLI's "single pane of glass":
- Show current actor, workspace name/path, base branch, and repo root.
- Show active leases owned by actor (and conflicts if any).
- Show protected path overrides for this workspace.
- Display warnings (e.g., uninitialized sv, missing .sv.toml, expired leases).

Why: a premium CLI gives users a quick, confidence‑building snapshot.


## Acceptance Criteria

sv status outputs a single cohesive summary: actor, workspace, base branch, active leases, protect overrides, and warnings; JSON output matches schema; supports --quiet/--json.


