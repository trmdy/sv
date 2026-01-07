---
id: sv-8jf.2.5
status: closed
deps: []
links: []
created: 2025-12-31T14:37:12.452253075+01:00
type: task
priority: 1
parent: sv-8jf.2
---
# sv ws rm: remove worktree and unregister

Implement sv ws rm <name> [--force]:
- Remove Git worktree (git worktree remove)
- Unregister from .git/sv/workspaces.json
- Do NOT delete commits (branches preserved unless explicit)
- --force to remove even with uncommitted changes
- Record operation in oplog for undo

Acceptance: sv ws rm agent1 removes worktree, can be undone


