---
id: sv-8jf.2.7
status: closed
deps: []
links: []
created: 2025-12-31T14:45:23.167953368+01:00
type: task
priority: 2
parent: sv-8jf.2
---
# sv status: current workspace overview

Implement sv status command for quick overview:
- Current workspace name and branch
- Actor identity
- Active leases held by current actor
- Protected paths that would block commit
- Ahead/behind vs base
- Any active conflicts or warnings

Convenience command combining ws info + lease ls + protect status

Acceptance: sv status shows comprehensive current state at a glance


