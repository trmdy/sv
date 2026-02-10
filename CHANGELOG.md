# Changelog

All notable changes to this project documented here.

## v0.2.4 - 2026-02-10

### Added
- Enforced exclusive `in_progress` task ownership across CLI and TUI start paths.
- Added explicit ownership transfer on start via `sv task start --takeover`.
- Made same-actor repeated start idempotent (no duplicate start events).

### Fixed
- Enforced mutual exclusivity between project-group links and parent/child links.
- Pruned legacy parent links targeting project-group tasks during relation rebuild.
- Added validation guards:
  - `sv task parent set` rejects project-group parents
  - `sv task project set` rejects legacy-task projects that already have children
  - TUI create/edit task flows enforce same constraints

### Docs
- Added start exclusivity PRD and review notes.
- Added gotchas for start takeover and project/parent exclusivity behavior.

### Tests
- Added coverage for start ownership conflict, takeover, idempotence, and parallel race.
- Added coverage for project-group/parent-link conflict prevention and pruning behavior.

## v0.2.3 - 2026-02-09

### Fixed
- Hardened task replay against duplicate `task_created` events; keep earliest create and ignore duplicates during fold/replay.
- Added task log diagnostics with `sv task doctor`:
  - duplicate `task_created` detection with kept/duplicate event ids
  - malformed JSONL line detection with path and line number
- Added repair command `sv task repair --dedupe-creates` with `--dry-run`.
- Added duplicate-create warning in `sv task sync` without hard-failing sync.

### Tests
- Added unit tests for duplicate-create tolerance and duplicate detection.
- Added integration tests covering:
  - corrupted task log tolerance (`task list/show/project set`)
  - `doctor` detection
  - `repair` dry-run + apply flow
  - malformed event reporting
  - `task sync` duplicate warning
