---
id: sv-8jf.6.1
status: closed
deps: []
links: []
created: 2025-12-31T14:38:45.26089426+01:00
type: task
priority: 0
parent: sv-8jf.6
---
# sv risk: basic overlap detection

Implement sv risk [-s <selector>] [--base <ref>]:
- For each workspace (or selected workspaces):
  - Compute touched files vs base: git diff --name-only <base>..<ws-branch>
- Intersect touched sets across workspaces
- Report overlap summary by file/directory
- Support --json output

Acceptance: sv risk shows file overlaps between active workspaces


