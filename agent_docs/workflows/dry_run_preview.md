# Dry-run / Preview UX (Design Note)

This note defines a future `--dry-run` / `--preview` flag for sv commands. It
focuses on user confidence: show what *would* happen, without making changes.

## Goals

- Provide a safe, confidence-building preview for actions that write state.
- Keep behavior consistent across commands and output modes.
- Preserve automation semantics (exit codes, JSON shape) in preview mode.

## Flag naming

- Primary: `--dry-run`.
- Alias: `--preview` (optional; same behavior).
- If both are supplied, treat as `--dry-run` (no special behavior).

## Global behavior

- No persistent changes: no writes to disk, no git operations that mutate state.
- All validation still runs (e.g., missing repo, invalid config, conflicts).
- Exit codes match the real command outcome:
  - success -> `0`
  - user error -> `2`
  - policy blocked -> `3`
  - operation failed -> `4`
- Human output uses the same structure with a preview header.
- JSON output matches the standard envelope with a preview hint.

### Human output convention

Header includes a preview marker:

```
sv <command>: preview
```

Summary should contain:

- `dry_run: true`
- `would_apply: true|false`

### JSON output convention

Add `data.preview` fields to `data`:

```json
{
  "schema_version": "sv.v1",
  "command": "take",
  "status": "success",
  "data": {
    "preview": {
      "dry_run": true,
      "would_apply": true
    },
    "...": "command-specific"
  },
  "warnings": [],
  "next_steps": []
}
```

If the preview indicates the real command would fail, return the standard
error envelope and include `preview.dry_run` in `error.details`:

```json
{
  "schema_version": "sv.v1",
  "command": "take",
  "status": "error",
  "error": {
    "message": "lease conflict: src/auth/token.rs held by bob",
    "code": 3,
    "kind": "policy_blocked",
    "details": {
      "preview": {"dry_run": true}
    }
  }
}
```

## Events integration

- Default: no events emitted during preview.
- If `--events` is provided with `--dry-run`, emit preview events with
  `dry_run: true` and `applied: false` so automation can distinguish them.

Example event:

```json
{"event":"lease_created","dry_run":true,"applied":false,"lease":{...}}
```

## Candidate commands and preview content

### `sv take --dry-run`

- Preview created leases and conflicts.
- Output shows:
  - leases that would be created
  - conflicts that would block creation
  - `would_apply` false when conflicts prevent any lease creation

### `sv release --dry-run`

- Preview which leases would be released (by id or pathspec).
- Output shows:
  - ids that would be released
  - ids not found or already inactive

### `sv protect add --dry-run`

- Preview config changes without writing `.sv.toml`.
- Output shows:
  - patterns to add
  - patterns already present
  - invalid patterns

### `sv protect off --dry-run`

- Preview workspace override changes without writing `.sv/` state.
- Output shows:
  - patterns that would be disabled
  - patterns not found in current protect rules

### `sv ws new --dry-run`

- Preview workspace path + branch creation.
- Output shows:
  - resolved workspace path (Git worktree path)
  - branch name and base ref
  - warnings if path exists or branch already exists

### `sv ws rm --dry-run`

- Preview removal of a workspace.
- Output shows:
  - workspace path (Git worktree path)
  - branch ref that would be removed (if applicable)
  - warnings if uncommitted changes would require `--force`

## Notes

- Preview should never mutate the oplog.
- Keep wording aligned with `agent_docs/workflows/cli_output.md`.
- Use "workspace" as the primary term ("worktree" only as a parenthetical).
