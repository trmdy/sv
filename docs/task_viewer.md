# Task Viewer (TUI) Specification

## Summary
- `sv task` launches a fullscreen TUI for browsing repo tasks.
- v1: read-only. v2: start/close/comment actions.
- Performance-first: no stutter, event-driven redraw, cached rendering.

## Goals
- Fast launch and smooth navigation for 1k-5k tasks.
- Clear split view: list left, details right.
- Minimal, predictable keymap.

## Non-goals (v1)
- No web UI.
- No kanban, graph, or analytics dashboards.
- No inline edits in TUI (v2 actions only).

## CLI entrypoint
- `sv task` with no subcommand launches the TUI.
- All existing subcommands remain unchanged for scripting.

## Layout
- Default: split view.
  - Left: task list.
  - Right: detail panel.
- Narrow terminals: single pane list, detail opens in place.
- Footer: key hints + status line (filter, errors, watch state).

## List row format
- Status pill, ID, title, workspace (if present), updated-at (optional).
- Sort: status rank -> updated_at desc -> id.
- Highlight selected row.

## Detail panel
- Title, status, timestamps, workspace/branch, actor if available.
- Body text, then comments (chronological).
- Markdown render optional; fallback to wrapped plain text.

## Keymap (v1)
- `j/k` or arrows: move selection.
- `enter`: toggle detail in narrow mode.
- `/`: start filter (fuzzy by id + title).
- `esc`: clear filter or close filter input.
- `o/p/c/a`: quick filter open / in_progress / closed / all.
- `r`: manual reload.
- `?`: help overlay.
- `q` or `ctrl+c`: quit.

## Data sources
- Prefer `.git/sv/tasks.snapshot.json`.
- Fallback to `.tasks/tasks.snapshot.json`.
- If no snapshot: fold log (warn in status line).
- Keep selection by task id on reload.

## File watch
- Watch:
  - `.tasks/tasks.jsonl`
  - `.tasks/tasks.snapshot.json`
  - `.git/sv/tasks.jsonl`
  - `.git/sv/tasks.snapshot.json`
- Debounce reload 150-300ms.
- Reload in background, then swap model on UI thread.
- Surface watch errors in status line; keep last good snapshot.

## Performance budgets
- Cold start: <150ms @1k tasks, <300ms @5k.
- Keypress-to-frame: <16ms.
- Reload: <50ms.

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

## Architecture (module layout)
- `src/ui/task_viewer/mod.rs`
- `src/ui/task_viewer/model.rs` (state, filters, selection)
- `src/ui/task_viewer/view.rs` (rendering)
- `src/ui/task_viewer/input.rs` (key handling)
- `src/ui/task_viewer/style.rs` (theme)
- `src/ui/task_viewer/cache.rs` (render caches)
- `src/ui/task_viewer/watch.rs` (file watch + reload)

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

## References
- Mario Zechner, "What I learned building an opinionated and minimal coding agent" (pi-tui retained mode + differential rendering).
