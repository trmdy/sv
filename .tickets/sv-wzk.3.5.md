---
id: sv-wzk.3.5
status: closed
deps: []
links: []
created: 2025-12-31T14:45:16.918193953+01:00
type: task
priority: 2
parent: sv-wzk.3
---
# sv hoist --continue-on-conflict flag

Implement --continue-on-conflict for sv hoist:
- On conflict: record conflicting commit, skip it, continue with rest
- Report all skipped commits at end
- Store conflict records in .git/sv/hoist/
- Allow user to resolve and re-run
- Clear summary of what succeeded vs failed

Per spec Section 11.1

Acceptance: sv hoist --continue-on-conflict skips conflicts and reports them


