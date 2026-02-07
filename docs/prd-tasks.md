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
- Full issue tracker (sprints, dashboards, advanced dependency graphs).
- Realtime cross-machine sync (future).
- Web UI or advanced visualizations (kanban, graph, insights).

## Users
- Multi-agent teams in one repo.
- CI/automation.

## UX principles
- Append-only event log.
- Deterministic output.
- Minimal flags.

## Data model (event log)
- Task ID: `<id_prefix>-<suffix>`, where suffix starts at `id_min_len` alphanum chars and grows as needed.
- Event ID: ULID per event (dedup, merge safety).
- Event types: `task_created`, `task_started`, `task_status_changed`, `task_priority_changed`, `task_edited`, `task_closed`, `task_deleted`, `task_commented`, `task_epic_set`, `task_epic_cleared`, `task_project_set`, `task_project_cleared`, `task_parent_set`, `task_parent_cleared`, `task_blocked`, `task_unblocked`, `task_related`, `task_unrelated`.
- Task state derived by folding events in order.

### Event fields (JSONL)
- `event_id`, `task_id`, `event_type`
- `related_task_id` (relations)
- `relation_description` (non-blocking relations)
- `timestamp`, `actor`
- `title`, `body` (create/edit)
- `status` (status change)
- `priority` (create/priority change, P0-P4)
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
id_min_len = 3
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
- `sv task` (launch fullscreen TUI)
- `sv task new <title> [--status <s>] [--priority <P0-P4>] [--body <txt>]`
- `sv task list [--status <s>] [--priority <P0-P4>] [--epic <id>] [--project <id>] [--workspace <name|id>] [--actor <name>] [--updated-since <rfc3339>] [--limit <n>] [--json]`
- `sv task ready [--priority <P0-P4>] [--epic <id>] [--project <id>] [--workspace <name|id>] [--actor <name>] [--updated-since <rfc3339>] [--limit <n>] [--json]`
- `sv task show <id> [--json]`
- `sv task start <id>`
- `sv task status <id> <status>`
- `sv task priority <id> <P0-P4>`
- `sv task edit <id> [--title <text>] [--body <text>]`
- `sv task close <id>`
- `sv task delete <id>`
- `sv task comment <id> <text>`
- `sv task parent set <child> <parent>`
- `sv task parent clear <child>`
- `sv task epic set <task> <epic>`
- `sv task epic clear <task>`
- `sv task project set <task> <project>`
- `sv task project clear <task>`
- `sv task block <blocker> <blocked>`
- `sv task unblock <blocker> <blocked>`
- `sv task relate <a> <b> --desc <text>`
- `sv task unrelate <a> <b>`
- `sv task relations <id>`
- `sv task sync`
- `sv task prefix [<prefix>]`

## Task viewer (TUI)
- v1: read-only viewer for list + detail.
- v2: start/close/comment actions.
- Keymap + perf budgets in `docs/task_viewer.md`.

## Workflows
- Create task -> status `default_status`.
- Create task -> priority `P2` unless specified.
- Start task -> status `in_progress_status`, attach workspace + branch (multiple tasks per workspace allowed).
- Close task -> status in `closed_statuses`, optional note.
- Ready task -> status `default_status` and no blockers.
- Relations: epic, project, parent, blocks, and described relations; use `sv task relations` to inspect.
- List/show prefers shared snapshot; falls back to fold log.
- Sync between participants: `git pull` brings `.tasks/*`, then `sv task sync` rebuilds snapshot + refreshes shared cache.
- Task IDs are case-insensitive; can be referenced by unique prefix of the suffix (e.g., `ab`, `a9`), or full ID (any prefix). Changing `id_prefix` does not affect existing tasks.

## Compaction
- Manual by default: `sv task compact`.
- Optional auto: if `[tasks.compaction] auto = true`, run during `sv task sync`.
- Policy: drop intermediate status events for closed tasks; keep create + latest status + latest edit + comments.

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
