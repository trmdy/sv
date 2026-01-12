# Task Viewer Test Plan

## Scope
Unit tests for pure logic and cache behavior. No heavy end-to-end UI tests.

## Unit test cases
### Filtering
- Match by id (full, prefix, case-insensitive).
- Match by title (case-insensitive, partial).
- Combined filter: text + status filter (AND).
- Empty filter returns all tasks.
- Filter with no results keeps selection = none.

### Sorting
- Status rank order (open < in_progress < closed < unknown).
- Updated_at desc within status.
- Tiebreaker by id for stable ordering.

### Selection persistence
- Selection preserved by task id across reload.
- If selected id missing, select first visible or none.
- Selection adjusts when filter changes to exclude selected.

### Cache invalidation
- Width change invalidates list + detail caches.
- Selection change re-renders only old + new rows.
- Task update invalidates only that task id.

### Data load order
- Prefer `.git/sv/tasks.snapshot.json`.
- Fallback to `.tasks/tasks.snapshot.json`.
- If snapshot missing, fold log and mark stale banner.

## Initial tests to implement first
- Filter by id + title (case-insensitive).
- Status rank ordering with tie by updated_at desc.
- Selection persistence by task id on reload.
- Cache invalidation on width change.

## Test harness notes
- Prefer pure functions in model layer for easy unit tests.
- Keep rendering tests minimal (string output length + sentinel text).
## Manual checks
- Resize behavior (split -> narrow).
- Live reload during sync.
- Large list scroll smoothness.
- Watch error banner and manual reload recovery.

## Notes
- Initial tests should live beside model logic (e.g., `src/ui/task_viewer/model.rs`).
