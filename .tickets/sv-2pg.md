---
id: sv-2pg
status: closed
deps: []
links: []
created: 2025-12-31T15:22:37.919804447+01:00
type: epic
priority: 1
parent: sv-8jf
---
# CLI UX & Workflow Polish

Background: The CLI currently mixes stub outputs, inconsistent messaging, and partial JSON support. To achieve a premium, Stripe-level UX, we need a consistent output contract, actionable guidance, and predictable error/exit semantics.\n\nGoals:\n- Consistent, human-friendly output across commands with clear headings and next steps.\n- Stable machine-readable JSON across all commands (schema versioned).\n- Predictable exit codes and error messaging that are automation-safe.\n- Terminology consistency (workspace vs worktree) and polished help text.\n\nNon-goals:\n- Implementing missing core functionality (these tasks focus on UX and presentation).\n- UI beyond CLI (no TUI/web).\n\nNotes:\n- Coordinate with in-progress sv-8jf.9.2 (help text) to avoid duplication.\n- Keep changes scoped and avoid refactors unrelated to UX output.\n


