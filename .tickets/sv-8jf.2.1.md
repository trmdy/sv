---
id: sv-8jf.2.1
status: closed
deps: []
links: []
created: 2025-12-31T14:37:02.934909388+01:00
type: task
priority: 0
parent: sv-8jf.2
---
# sv ws new: create worktree with branch and registry entry

Implement sv ws new <name> [--base <ref>] [--dir <path>] [--branch <ref>] [--sparse <pathspec...>]:
- Create Git worktree directory
- Create branch (default: sv/ws/<name>)
- Register in .git/sv/workspaces.json
- Support --base to specify starting point (default from .sv.toml)
- Support --dir to specify custom directory path
- Support --sparse for sparse checkout (optional v0.1)

Acceptance: sv ws new agent1 creates worktree, branch sv/ws/agent1, registry entry


