---
id: sv-8jf.2.3
status: closed
deps: []
links: []
created: 2025-12-31T14:37:07.838415923+01:00
type: task
priority: 1
parent: sv-8jf.2
---
# sv ws list: enumerate workspaces with status

Implement sv ws list [-s <selector>]:
- List all registered workspaces
- Show: name, path, branch, base, actor (if set), ahead/behind status, last activity
- Support --json output
- Selector filtering deferred to selector epic (stub for now)

Acceptance: sv ws list shows all workspaces with accurate status


