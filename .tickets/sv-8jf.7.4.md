---
id: sv-8jf.7.4
status: closed
deps: []
links: []
created: 2025-12-31T14:39:11.113276031+01:00
type: task
priority: 1
parent: sv-8jf.7
---
# Undoable operation recording for ws/lease commands

Ensure all major commands record undo information:
- sv ws new: record worktree path, branch ref
- sv ws rm: record branch ref (worktree can't be restored)
- sv take: record created lease ids
- sv release: record released lease state
- sv commit: record old/new HEAD, Change-Id

Acceptance: sv undo works for all documented operations


