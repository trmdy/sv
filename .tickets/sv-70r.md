---
id: sv-70r
status: open
deps: []
links: []
created: 2026-01-07T19:56:30.808910886+01:00
type: feature
priority: 2
---
# Add lifecycle hooks for worktree operations

Add lifecycle hooks for worktree operations: post-create/start/switch, pre-commit/merge, post-merge, pre-remove; use minijinja-style templating + JSON context; support project hooks (repo config) with approval gating and user hooks without approval; log background hook output; hooks opt-in and must not alter existing flows unless configured.


