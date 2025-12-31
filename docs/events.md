# Event Output

sv can emit JSONL events for external integrations. Each line is a single JSON
object. Event output is optional and is only enabled when `--events` is set.

## Enabling events

- `sv <cmd> --events` emits JSONL to stdout.
- `sv <cmd> --events <path>` appends JSONL to the given file.
- `sv <cmd> --events -` is an explicit stdout form.

Note: when events are written to stdout, sv suppresses normal output to avoid
mixing formats. Use `--events <path>` if you want the usual command output.

## Event envelope

```json
{
  "schema_version": "sv.event.v1",
  "event": "lease_created",
  "timestamp": "2025-01-01T00:00:00Z",
  "actor": "alice",
  "data": {
    "id": "...",
    "pathspec": "src/auth/**",
    "strength": "cooperative",
    "intent": "feature",
    "scope": "repo",
    "ttl": "2h",
    "expires_at": "2025-01-01T02:00:00Z",
    "note": "..."
  }
}
```

Fields:

- `schema_version`: event schema identifier (currently `sv.event.v1`).
- `event`: snake_case event name.
- `timestamp`: RFC3339 UTC timestamp.
- `actor`: optional actor identity.
- `data`: event-specific payload (optional).

## Event kinds

- `lease_created`: emitted after a lease is created.
- `lease_released`: emitted after a lease is released.
- `workspace_created`: emitted after a workspace is created.
- `workspace_removed`: emitted after a workspace is removed.
- `commit_blocked`: emitted when a commit is blocked by policy.
- `commit_created`: emitted after a successful `sv commit`.

As of v0.1, `sv take` emits `lease_created` and `sv release` emits
`lease_released`. Other event kinds will be wired as their commands are
finalized.

## Payloads

Payloads are event-specific and may be omitted. Example payload for
`lease_created`:

```json
{
  "schema_version": "sv.event.v1",
  "event": "lease_created",
  "timestamp": "2025-01-01T12:00:00Z",
  "actor": "alice",
  "data": {
    "id": "7b0f6e2e-4b0e-4d3a-9e71-2f8b8c29f4e2",
    "pathspec": "src/auth/**",
    "strength": "cooperative",
    "intent": "feature",
    "scope": "repo",
    "actor": "alice",
    "ttl": "2h",
    "expires_at": "2025-01-01T14:00:00Z",
    "created_at": "2025-01-01T12:00:00Z",
    "note": "Work on auth flow"
  }
}
```

`lease_released` payloads include the release timestamp:

```json
{
  "schema_version": "sv.event.v1",
  "event": "lease_released",
  "timestamp": "2025-01-01T12:30:00Z",
  "actor": "alice",
  "data": {
    "id": "7b0f6e2e-4b0e-4d3a-9e71-2f8b8c29f4e2",
    "pathspec": "src/auth/**",
    "strength": "cooperative",
    "intent": "feature",
    "scope": "repo",
    "actor": "alice",
    "ttl": "2h",
    "expires_at": "2025-01-01T14:00:00Z",
    "released_at": "2025-01-01T12:30:00Z",
    "note": "Work on auth flow"
  }
}
```

## Stability

The envelope fields are intended to remain stable across v0.x. Event payloads
may grow over time; consumers should ignore unknown fields.
