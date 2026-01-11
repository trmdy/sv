---
id: sv-8jf.1.6
status: closed
deps: []
links: []
created: 2025-12-31T14:36:44.421355921+01:00
type: task
priority: 1
parent: sv-8jf.1
---
# Git integration: libgit2 wrapper for repo discovery and operations

Implement core Git integration layer:
- Repo discovery (find .git from cwd or --repo)
- Repository validation and error handling
- Common Git state queries (current branch, HEAD, etc.)

NOTE: This is the foundation. Worktree, branch, diff, and commit operations are separate tasks.

Acceptance: sv correctly discovers and validates Git repository


