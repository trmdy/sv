# Task Viewer (TUI) Specification

## Summary
- `sv task` launches a fullscreen TUI for browsing repo tasks.
- v1: read-only. v2: create/edit/priority actions.
- Performance-first: no stutter, event-driven redraw, cached rendering.

## Goals
- Fast launch and smooth navigation for 1k-5k tasks.
- Clear split view: list left, details right.
- Minimal, predictable keymap.

## Non-goals (v1)
- No web UI.
- No kanban, graph, or analytics dashboards.

## CLI entrypoint
- `sv task` with no subcommand launches the TUI.
- `sv task --epic <id>` (or `SV_EPIC=<id>`) starts with epic filter applied.
- All existing subcommands remain unchanged for scripting.

## Layout
- Default: split view.
  - Left: task list.
  - Right: detail panel.
- Narrow terminals: single pane list, detail opens in place.
- Footer: key hints + status line (filter, epic filter, errors, watch state) + task counts.
  - Tasks mode counts: open/ready/closed (`ready = open + not blocked`).
  - Epics mode counts: current/completed epics.

## Wireframes
Split view (default):
```
┌ Tasks ───────────────────────────┐┌ Task sv-1a2 ───────────────┐
│ [open] sv-1a2 Fix sync edge case ││ Status: open              │
│ [prog] sv-1a3 TUI spec draft     ││ Updated: 2025-01-12       │
│ [open] sv-1a4 Add watch debounce ││ Workspace: ws-dev         │
│                                  ││ Branch: feat/task-tui     │
│                                  ││                          │
│ / filter: tui                    ││ Body...                  │
│                                  ││                          │
└──────────────────────────────────┘└───────────────────────────┘
     j/k move  n new  e edit  p priority  / filter  r reload  q quit   filter:tui
```

Narrow view:
```
┌ Tasks ─────────────────────────────┐
│ [open] sv-1a2 Fix sync edge case   │
│ [prog] sv-1a3 TUI spec draft       │
│ [open] sv-1a4 Add watch debounce   │
│                                    │
└────────────────────────────────────┘
     enter: details  / filter  q quit
```

## List row format
- Status pill, ID, title, workspace (if present), updated-at (optional).
- Sort: status rank -> priority rank -> readiness -> updated_at desc -> id.
- Readiness: default status and not blocked.
- Parent nesting: children render directly under parent with indentation.
- Highlight selected row.
- Subtle ready marker for open + unblocked tasks.

## Detail panel
- Title, status, timestamps, workspace/branch, actor if available.
- Body text, then comments (chronological).
- Markdown render optional; fallback to wrapped plain text.

## Keymap (v1)
- `j/k` or arrows: move selection.
- `enter`: toggle detail in narrow mode.
- `/`: start filter (fuzzy by id + title).
- `esc`: clear filter or close filter input.
- `o/i/c/a`: quick filter open / in_progress / closed / all.
- `r`: manual reload.
- `x`: epic filter picker.
- `v`: toggle list mode (tasks/epics).
- `n`: new task wizard.
- `e`: inline edit task.
- `p`: change task priority.
- `q` or `ctrl+c`: quit.

## Keymap table (v1)
| Key | Action |
| --- | --- |
| `j` / `k` | Move selection |
| `↑` / `↓` | Move selection |
| `enter` | Toggle detail (narrow) |
| `/` | Focus filter |
| `x` | Epic filter picker |
| `v` | Toggle tasks/epics view |
| `esc` | Clear filter |
| `o` | Filter open |
| `i` | Filter in_progress |
| `c` | Filter closed |
| `a` | Clear status filter |
| `r` | Reload |
| `n` | New task wizard |
| `e` | Inline edit |
| `p` | Change priority |
| `q` / `ctrl+c` | Quit |

## Filter behavior
- Fuzzy match on id + title (case-insensitive).
- While filter active, show input line under list header.
- If filter yields no results, show "No matches".
- Status filter and epic filter combine with text filter (AND).
- Clear filter resets selection to first visible item.

## Data sources
- Prefer `.git/sv/tasks.snapshot.json`.
- Fallback to `.tasks/tasks.snapshot.json`.
- If no snapshot: fold log (warn in status line).
- Keep selection by task id on reload.
- Sort: status rank -> priority rank -> readiness -> updated_at desc -> id.

### Load order (pseudocode)
```
if shared_snapshot exists:
  load shared_snapshot
else if tracked_snapshot exists:
  load tracked_snapshot
else if shared_log exists:
  fold shared_log (warn)
else if tracked_log exists:
  fold tracked_log (warn)
else:
  empty list
```

### Snapshot freshness
- Prefer newest snapshot if both shared/tracked exist.
- If snapshot older than log mtime, reload from log in background, then swap.
- Show "stale data" banner until fresh snapshot loaded.

## File watch
- Watch:
  - `.tasks/tasks.jsonl`
  - `.tasks/tasks.snapshot.json`
  - `.git/sv/tasks.jsonl`
  - `.git/sv/tasks.snapshot.json`
- Debounce reload 150-300ms.
- Reload in background, then swap model on UI thread.
- Surface watch errors in status line; keep last good snapshot.

### Watch pipeline
1) `notify` emits event -> debounce timer reset.
2) Timer fires -> loader thread pulls newest snapshot/log.
3) Loader sends `UiMsg::DataLoaded` with snapshot + metadata.
4) UI swaps state, preserves selection, refreshes caches.

### Reload rules
- If multiple events within debounce window, do single reload.
- If load fails, keep last good snapshot and show error banner.
- Manual reload (`r`) bypasses debounce.

## Performance budgets
- Cold start: <150ms @1k tasks, <300ms @5k.
- Keypress-to-frame: <16ms.
- Reload: <50ms.

## Performance risks + mitigations
- Large snapshots (10k+ tasks) cause parse spikes.
  - Mitigation: background loader + incremental UI swap; show "loading" banner.
- Markdown render cost for long bodies.
  - Mitigation: cache rendered body per `(task_id, width)`; fallback to plain wrap if >N ms.
- Frequent file changes (sync bursts) cause churn.
  - Mitigation: debounce watch (150-300ms) + coalesce reloads.
- Resize storms from terminal.
  - Mitigation: debounce resize events to 16-32ms; re-render only if width/height change.
- Slow disk or networked repo.
  - Mitigation: snapshot-first + avoid log fold on startup; show stale data with warning.

## Profiling spec
- Env: `SV_TASK_PROFILE=1`
- Emit timings (stderr):
  - `load_snapshot_ms`
  - `parse_ms`
  - `sort_ms`
  - `filter_ms`
  - `render_ms`
  - `frame_ms`
- Optional `SV_TASK_PROFILE=2`:
  - per-component render timing (list, detail, footer)
  - cache hit/miss counters

## Performance strategy
- Event-driven redraw only (input, resize, data change). No idle ticks.
- Retained caches for list rows and detail bodies.
- Differential rendering: update only changed lines.
- Virtualization: render only visible list window and detail viewport.
- Background loader thread; UI thread never blocks on disk or parse.
- Incremental filter: precompute normalized id/title.

## Caching and invalidation
- List row cache key: `(task_id, width, selected)`.
- Detail cache key: `(task_id, width)`.
- Invalidate on width change, selection change, or task change.
- Markdown render cached per `(task_id, width)`.

## Rendering + virtualization details
- Compute visible list window from selection + viewport height.
- Render only rows in window; keep list width fixed for stable diffing.
- Detail viewport uses vertical scroll with internal offset (no terminal scroll).
- Truncate long lines with ellipsis; keep raw for copy/export later.
- Status pills are fixed-width to avoid layout jitter.

### Cache invalidation rules
- Width/height change: drop list + detail caches.
- Selection change: re-render old + new selected rows only.
- Filter change: invalidate list cache for non-visible rows.
- Task update: invalidate cached entries for that task id.
## Architecture (module layout)
- `src/ui/task_viewer/mod.rs`
- `src/ui/task_viewer/model.rs` (state, filters, selection)
- `src/ui/task_viewer/view.rs` (rendering)
- `src/ui/task_viewer/input.rs` (key handling)
- `src/ui/task_viewer/style.rs` (theme)
- `src/ui/task_viewer/cache.rs` (render caches)
- `src/ui/task_viewer/watch.rs` (file watch + reload)

### Data flow
```
TaskStore -> Loader thread -> TaskSnapshot -> UI model -> Render tree
                       (debounced fs events)          (cached)
```

### Core structs
```
struct UiState {
  tasks: Vec<TaskRecord>,
  filtered: Vec<usize>,
  selected: Option<usize>,
  filter: String,
  status_filter: Option<String>,
  view_mode: ViewMode,
  load_state: LoadState,
  cache: RenderCache,
}

struct RenderCache {
  list_rows: HashMap<(String, u16, bool), String>,
  detail: HashMap<(String, u16), Vec<String>>,
  markdown: HashMap<(String, u16), Vec<String>>,
  hits: u64,
  misses: u64,
}

enum UiMsg {
  DataLoaded(TaskSnapshot),
  LoadError(String),
  WatchError(String),
  Key(KeyEvent),
  Resize(u16, u16),
}
```

### Threading
- UI thread: input + render; no IO.
- Loader thread: snapshot load + parse + sort + filter + send `UiMsg::DataLoaded`.
- Watch thread: debounced `notify`, triggers loader.

### Error handling
- Load error keeps last good snapshot.
- Watch error shown in status line; manual reload still works.

## Telemetry and profiling
- Env flag: `SV_TASK_PROFILE=1`.
- Log timings: load, parse, sort, render.
- Emit to stderr only in TUI mode.

## Error states
- Empty list.
- Load error (file missing, parse error).
- Watch error (fallback to manual reload).
- No selection (when list empty or filter excludes all).

## Testing plan
- Unit tests:
  - Filter matching.
  - Sort order.
  - Selection persistence across reload.
  - Cache invalidation on width change.
- Manual checks:
  - Resize behavior.
  - Live reload.
  - Large task list scroll performance.

## v2 actions (future)
- Start, close, comment via existing `TaskStore` ops.
- Confirm status transitions and actor handling.
- Add confirmation prompt for close if task has open comments.

### v2 keymap
| Key | Action |
| --- | --- |
| `s` | Start task |
| `x` | Close task |
| `c` | Add comment |

### v2 action flows
- Start:
  - Requires workspace info; if missing, show error banner.
  - Emits `TaskStarted` with workspace id/name + branch.
  - Sets status to `in_progress_status`.
- Close:
  - Confirmation prompt (yes/no).
  - Uses first `closed_statuses` unless user picks another.
  - Emits `TaskClosed` with status + actor.
- Comment:
  - Opens inline input; fallback to `$EDITOR` when long.
  - Emits `TaskCommented` with actor.

### Safety + errors
- Actions never mutate without explicit confirmation (close).
- Errors shown in status line; UI stays open.

## References
- Mario Zechner, "What I learned building an opinionated and minimal coding agent" (pi-tui retained mode + differential rendering).
