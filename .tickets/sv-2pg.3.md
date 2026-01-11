---
id: sv-2pg.3
status: closed
deps: [sv-2pg.1, sv-2pg.2]
links: []
created: 2025-12-31T15:23:36.21976276+01:00
type: task
priority: 1
parent: sv-2pg
---
# Error/exit consistency + single error emitter

Unify error handling so that:
- Commands return Result without printing success on error.
- CLI entrypoint handles formatting and exit codes (2/3/4) in one place.
- JSON mode emits a single structured error object (schema in sv-2pg.1).

Why: prevents confusing mixed output and makes automation reliable.


## Acceptance Criteria

All commands return Result without printing success on error; errors are emitted once (human or JSON) with correct exit code; no command returns Ok(()) after reporting a conflict or error.


