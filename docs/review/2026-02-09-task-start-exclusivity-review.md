# Review: Task Start Exclusivity (2026-02-09)

Status update (same day): findings below were fixed in current working tree by introducing guarded `start_task` flow, `--takeover`, TUI parity routing, and regression tests.

## Findings

1. Critical: CLI `task start` still allows cross-actor takeover without consent.
   - `run_start` appends `TaskStarted` unconditionally; no ownership guard.
   - Ref: `src/cli/task.rs:794`, `src/cli/task.rs:810`
   - Repro (2026-02-09):
     - `sv --actor alice task start <id>` -> exit 0
     - `sv --actor bob task start <id>` -> exit 0 (should fail without explicit takeover)
     - `task show` ends with `started_by=bob` (last writer wins)

2. High: `--takeover` not implemented in CLI surface.
   - Start command only accepts `id`; no flag.
   - Start options carry no takeover boolean.
   - Ref: `src/cli/mod.rs:1215`, `src/cli/task.rs:90`

3. High: same-actor idempotence not implemented.
   - Repeated `sv --actor alice task start <id>` emits duplicate `task_started`.
   - Repro (2026-02-09): `events=3` after create + two starts (expected no-op on second start).

4. Medium: TUI start path not aligned with exclusivity design.
   - TUI status picker applies `change_status` directly (`TaskStatusChanged`) rather than guarded `task start` flow.
   - Ref: `src/ui/task_viewer/app.rs:986`, `src/ui/task_viewer/app.rs:992`

5. Medium: missing regression coverage for exclusivity acceptance criteria.
   - No tests found for:
     - actor A start, actor B start fails
     - `--takeover` success path
     - same-actor no-op start
     - concurrent start race single winner
   - Existing start-related grep hit is hook setup comment only.
   - Ref: `tests/forge_hooks.rs:45`

## Scope checked

- Code paths: CLI start command, task event application, TUI start/status path.
- Validation: `cargo check`; `cargo test --test task_project_cli`; `cargo test task_started` (filter run; no exclusivity tests executed because none exist).
