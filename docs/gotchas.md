# Gotchas / Pitfalls

Status: evolving
Last verified: 2026-02-09

Add short entries that prevent repeated failures.

## Format
- **Symptom:** what you see
- **Cause:** why it happens
- **Fix:** exact commands / code pointers

## Entries
- **Symptom:** build fails at link time with OpenSSL errors like `found architecture 'arm64', required architecture 'x86_64'` or `_OPENSSL_init_ssl` missing.
  **Cause:** x86_64 Rust toolchain running on Apple Silicon while Homebrew OpenSSL is arm64 (`/opt/homebrew`).
  **Fix:** use a native arm64 Rust toolchain (e.g., `rustup default stable-aarch64-apple-darwin`) or install x86_64 Homebrew OpenSSL under `/usr/local` and point `OPENSSL_DIR`/`PKG_CONFIG_PATH` there when using the x86_64 toolchain. A macOS preflight now fails early when it detects `/opt/homebrew` for x86_64 builds.
- **Symptom:** after `sv hoist` on the current branch, `git status` shows widespread deletions or missing files even though HEAD advanced.
  **Cause:** worktree checkout skipped because local changes blocked a safe checkout or checkout failed.
  **Fix:** clean local changes and run `git restore --source=HEAD --staged --worktree .` (or rerun hoist with a clean worktree).
- **Symptom:** `sv task list/show/project set` fails or behaves inconsistently after log corruption with repeated `task_created` for one task.
  **Cause:** duplicate `task_created` events in `.tasks/tasks.jsonl` (same `task_id`, different `event_id`).
  **Fix:** run `sv task doctor`; preview cleanup with `sv task repair --dedupe-creates --dry-run`; apply with `sv task repair --dedupe-creates`; rerun `sv task doctor` to confirm clean log.
- **Symptom:** task appears indented as a child under a project grouping task in TUI.
  **Cause:** legacy `task_parent_set` links targeting a task later used as a project group.
  **Fix:** now blocked for new writes (`sv task parent set` / TUI edit). Legacy links are ignored in relation resolution; optionally clear old links explicitly with `sv task parent clear <child>`.
- **Symptom:** `sv task start <id>` fails with `task already in progress by <actor>; use --takeover to transfer ownership`.
  **Cause:** start ownership is now exclusive; another actor already owns the in-progress task.
  **Fix:** use `sv task start <id> --takeover` to transfer ownership, or coordinate with current owner. Repeated start by the same actor is now a no-op.
