---
id: sv-wzk.3.6
status: closed
deps: [sv-wzk.3.1]
links: []
created: 2025-12-31T14:46:17.421879647+01:00
type: task
priority: 1
parent: sv-wzk.3
---
# Hoist commit selection from workspaces

Implement commit selection for hoist:
- Apply selector to identify source workspaces
- For each workspace, identify commits ahead of base
- Apply ordering mode (workspace/time/explicit)
- Build ordered commit list for replay

Depends on: selector language implementation

Acceptance: sv hoist correctly identifies commits to include from selected workspaces


