---
id: sv-wzk.3.2
status: closed
deps: []
links: []
created: 2025-12-31T14:40:00.05454519+01:00
type: task
priority: 2
parent: sv-wzk.3
---
# sv hoist: ordering modes

Implement hoist ordering modes --order <mode>:
- workspace (default): stable sort by workspace name, preserve commit order within each
- time: sort by commit timestamp
- explicit: take ordered list of workspaces or config-defined priority

Per spec Section 11.2

Acceptance: sv hoist --order workspace produces deterministic commit order


