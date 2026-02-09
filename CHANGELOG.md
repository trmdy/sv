# Changelog

All notable changes to this project documented here.

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
