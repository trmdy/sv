# PRD: Exclusive Task Start Ownership

## Problem
`sv task start <id>` currently allows multiple actors to start the same task concurrently.
This causes duplicate in-progress ownership and coordination thrash.

Observed behavior:
- Multiple `task_started` events for same `task_id`, different `actor`.
- Last writer wins for `started_by`/`updated_by` in snapshot, hiding overlap.

Current code path (no guard):
- `src/cli/task.rs` `run_start` appends `TaskStarted` unconditionally.
- `src/task.rs` `TaskEventType::TaskStarted` mutates record with no ownership conflict check.

## Goal
Enforce single active owner per task by default.
A task already in `in_progress` cannot be started by another actor unless explicitly taking over.

## Non-goals
- Prevent one actor from owning multiple tasks.
- Introduce full lock/lease system for tasks.
- Rewrite existing historical events.

## Proposal

### 1) Guard on `sv task start`
Before appending `TaskStarted`, resolve current task state.
If task status is `in_progress` and `started_by` is set to a different actor, fail with clear error.

Error message (example):
`task already in progress by <actor>; use --takeover to transfer ownership`

### 2) Explicit takeover
Add `--takeover` flag to `sv task start`.
Behavior with flag:
- If task is in progress by another actor, append a `TaskStarted` event and transfer ownership.
- Emit warning/summary showing previous owner -> new owner.

Optional (same PR or follow-up): `--reason <text>` and auto-comment for audit trail.

### 3) No-op start by same actor
If task already in progress by same actor:
- default behavior: success no-op (do not append duplicate `TaskStarted`).
- print summary: `already in progress by you`.

This prevents event spam from repeated `start` calls in loops.

### 4) TUI parity
Task viewer start action must follow the same rule:
- blocked when owned by another actor unless explicit takeover action.

### 5) All start entry points share same guard
Exclusivity must be enforced everywhere a start can happen:
- `sv task start <id>`
- `sv task pick` when it auto-starts a selected task
- task viewer start actions
- integration-triggered starts that call CLI start flow (for example forge hooks)

Single guard function in core task service layer; avoid duplicating logic across call sites.

## CLI UX

### Default
`sv task start sv-abc`
- succeeds if not in-progress by another actor.
- fails if owned by another actor.

### Takeover
`sv task start sv-abc --takeover`
- succeeds and transfers ownership.

### JSON errors
Maintain stable machine-readable error shape (`kind=user_error`, actionable message).

## Data model impact
No schema changes required.
Use existing `task_started` event semantics.
(Ownership is derived from latest valid start event.)

## Implementation notes
- Apply optimistic check + append under existing task log lock to avoid race windows.
- Return same user-error shape from all entry points.
- Keep same-actor re-start idempotent to reduce loop noise.
- Keep takeover explicit and auditable (stdout + optional task comment).

## Compatibility
- Existing logs remain valid.
- Existing multi-start history still readable.
- Behavior change is only on future `task start` commands.

## Acceptance criteria
1. Starting an open task works.
2. Starting an in-progress task owned by another actor fails by default.
3. Starting same task with `--takeover` succeeds and updates `started_by`.
4. Starting same task by same actor is idempotent (no duplicate event).
5. TUI start behavior matches CLI rules.
6. Parallel-start race test: only one actor wins without `--takeover`.

## Testing
- Unit tests:
  - guard logic for start ownership decisions.
  - idempotent same-actor start.
- Integration tests:
  - actor A start, actor B start -> fail.
  - actor B start `--takeover` -> success.
  - concurrent starts from two actors -> one success, one ownership error.

## Rollout
1. Implement CLI/TUI guard + `--takeover`.
2. Route `task pick` auto-start through same guard.
3. Add tests.
4. Update `docs/prd-tasks.md` workflow text.
5. Add short note in `docs/gotchas.md` (task ownership + takeover).
