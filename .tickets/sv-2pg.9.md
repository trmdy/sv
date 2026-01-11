---
id: sv-2pg.9
status: closed
deps: [sv-2pg.1, sv-8jf.9.2]
links: []
created: 2025-12-31T15:24:16.07685674+01:00
type: task
priority: 2
parent: sv-2pg
---
# Terminology consistency pass (workspace vs worktree)

Audit and normalize terminology across help text and outputs:
- Choose primary term (recommend: "workspace").
- Update CLI help strings, command outputs, docs, and errors.
- Ensure any mention of "worktree" is secondary/parenthetical.

Why: consistent language lowers confusion and feels more premium.


## Acceptance Criteria

All user-facing text uses a single primary term (workspace), with worktree only as a parenthetical; help text and outputs updated; no mixed terms in the same command output.


