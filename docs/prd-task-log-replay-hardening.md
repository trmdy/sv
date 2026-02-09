# PRD: Task Log Replay Hardening

## Problem
A single duplicate `task_created` event for the same `task_id` (with a different `event_id`) can break task replay.
Current behavior fails hard with:
`Invalid argument: task already exists: <task_id>`
This blocks normal commands like `sv task show`, `sv task list`, and `sv task project set`.

## Goal
Make `sv` resilient to corrupted/duplicate create events so task commands keep working and operators can repair logs safely.

## Non-goals
- Full log migration framework.
- Changing event schema.

## Proposal
1. Replay guard
- In task fold/apply path, detect duplicate `task_created` for existing `task_id`.
- Default behavior: keep first valid create (by `timestamp,event_id` order), ignore later duplicate create events.
- Emit warning metadata (count + affected task ids).

2. Validation command
- Add `sv task doctor`.
- Report:
  - duplicate `task_created` by `task_id`
  - malformed events
  - orphan relation events (optional)
- Support `--json` for automation.

3. Repair command
- Add `sv task repair --dedupe-creates`.
- Rewrites tracked log (`.tasks/tasks.jsonl`) to remove duplicate `task_created` events while preserving order of remaining events.
- Then runs snapshot rebuild.
- Dry-run mode first (`--dry-run`) with change summary.

4. Sync safety
- During `sv task sync`, run lightweight validation and print warning when duplicates are detected.
- Do not hard-fail sync on duplicate creates; keep repo usable.

## Acceptance criteria
- With duplicate `task_created` present, `sv task show/list/project set` still succeed.
- `sv task doctor` reports exact offending event ids and task ids.
- `sv task repair --dedupe-creates --dry-run` shows deterministic plan.
- `sv task repair --dedupe-creates` fixes log and clears doctor warning.
- Existing valid logs behave unchanged.

## Implementation notes
- Reuse existing merge/sort ordering: `timestamp`, then `event_id`.
- Keep first create event as canonical for `title/body/status/priority/created_by/created_at`.
- Add tests:
  - unit test for duplicate create tolerance
  - integration test: corrupted log -> doctor detects -> repair fixes -> commands succeed

## Rollout
1. Implement replay guard + tests.
2. Add `doctor` read-only detection.
3. Add `repair` with dry-run and write path.
4. Document in `docs/gotchas.md` + README task troubleshooting section.
