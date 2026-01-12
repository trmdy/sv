# PRD: sv Tasks

## Summary
- Add built-in task manager to sv, similar to `ticket`, repo-scoped.
- Committable task history + snapshot in `.tasks/`, worktree-synced via `.git/sv/`.
- Configurable statuses in `.sv.toml`.

## Goals
- Committable, mergeable task history.
- Worktree sync by default (no manual sync).
- Fast list/read/write.
- Unaffected by sv leases/protected paths.
- Simple CLI, scriptable JSON.

## Non-goals
- Full issue tracker (epics, sprints, dashboards).
- Realtime cross-machine sync (future).
- Rich UI.

## Users
- Multi-agent teams in one repo.
- CI/automation.

## UX principles
- Append-only event log.
- Deterministic output.
- Minimal flags.

## Data model (event log)
- Task ID: `<id_prefix>-<suffix>`, where suffix starts at 3 alphanum chars and grows as needed.
- Event ID: ULID per event (dedup, merge safety).
- Event types: `task_created`, `task_started`, `task_status_changed`, `task_closed`, `task_commented`.
- Task state derived by folding events in order.

### Event fields (JSONL)
- `event_id`, `task_id`, `event_type`
- `timestamp`, `actor`
- `title`, `body` (create)
- `status` (status change)
- `workspace`, `branch` (start/close)
- `comment` (comment)

## Storage
- Tracked (committable): `.tasks/tasks.jsonl` (append-only; may be rewritten by sync).
- Tracked snapshot: `.tasks/tasks.snapshot.json` (derived, can be regenerated).
- Shared (worktree sync): `.git/sv/tasks.jsonl` (append-only cache).
- Shared snapshot: `.git/sv/tasks.snapshot.json` (derived cache for fast reads).
- `sv task sync` merges + dedups logs (event_id), rewrites tracked log and snapshot in stable order.

## Config (.sv.toml)
```toml
[tasks]
id_prefix = "sv"
statuses = ["open", "in_progress", "closed"]
default_status = "open"
in_progress_status = "in_progress"
closed_statuses = ["closed"]

[tasks.compaction]
auto = false
max_log_mb = 200
older_than = "180d"
```

## CLI (initial)
- `sv task new <title> [--status <s>] [--body <txt>]`
- `sv task list [--status <s>] [--json]`
- `sv task show <id> [--json]`
- `sv task start <id>`
- `sv task status <id> <status>`
- `sv task close <id>`
- `sv task comment <id> <text>`
- `sv task sync`
- `sv task prefix [<prefix>]`

## Workflows
- Create task -> status `default_status`.
- Start task -> status `in_progress_status`, attach workspace + branch (multiple tasks per workspace allowed).
- Close task -> status in `closed_statuses`, optional note.
- List/show prefers shared snapshot; falls back to fold log.
- Sync between participants: `git pull` brings `.tasks/*`, then `sv task sync` rebuilds snapshot + refreshes shared cache.
- Task IDs are case-insensitive; can be referenced by unique prefix of the suffix (e.g., `ab`, `a9`), or full ID (with or without `id_prefix-`).

## Compaction
- Manual by default: `sv task compact`.
- Optional auto: if `[tasks.compaction] auto = true`, run during `sv task sync`.
- Policy: drop intermediate status events for closed tasks; keep create + latest status + comments.

## Hoist / commit integration
- `sv hoist` runs `sv task sync` pre-flight.
- If workspace has in-progress tasks, warn + hint: `sv task close <id>` or `sv hoist --close-tasks` (closes all active tasks for this workspace).
- `sv commit` no extra gating; `.tasks/` should not be protected by default.

## Concurrency + merge
- Writes use file lock for atomic append only (no sv lease/protect checks).
- Merge conflicts resolved by `sv task sync` (event_id dedup).
- Stable ordering: `timestamp`, then `event_id`.

## Performance
- Append-only writes.
- List uses snapshot if available, else fold events.
- O(n) fold, expected small n; snapshot can be rebuilt.

## Testing
- Unit: config parsing, status validation, event fold, dedup.
- Integration: concurrent appends, sync dedup, worktree visibility.

## Open questions
- Track labels/assignees in v1?
