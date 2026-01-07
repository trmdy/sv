---
id: sv-8jf.7.3
status: closed
deps: []
links: []
created: 2025-12-31T14:39:07.896627655+01:00
type: task
priority: 0
parent: sv-8jf.7
---
# sv undo: basic undo functionality

Implement sv undo [--op <id>]:
- Without --op: undo most recent undoable operation
- With --op: undo specific operation
- Undo semantics:
  - Ref moves: restore to previous tips
  - Workspace create: remove worktree (optionally --keep-worktree)
  - Workspace remove: error (can't restore deleted worktree, only branches)
  - Lease changes: restore previous state

Per spec Section 14.2

Acceptance: sv undo reverses last operation, shows what was undone


