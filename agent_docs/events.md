# sv events (JSONL)

sv can emit structured events for external integrations via the global
`--events` flag. The output is JSON Lines (one JSON object per line).

## Usage

```
sv --events - take src/auth/** --strength cooperative
sv --events /tmp/sv-events.jsonl take src/auth/**
```

Notes:
- Omit `--events` to disable event output.
- `--events` with no value defaults to stdout (`-`).
- Events are appended when writing to a file.

## Event envelope (schema version `sv.event.v1`)

Each event is a JSON object with this envelope:

```json
{
  "schema_version": "sv.event.v1",
  "event": "lease_created",
  "timestamp": "2025-01-01T12:00:00Z",
  "actor": "alice",
  "data": {}
}
```

Fields:
- `schema_version`: string, currently `sv.event.v1`.
- `event`: string enum (snake_case).
- `timestamp`: RFC3339 UTC timestamp.
- `actor`: optional string (omitted when unknown).
- `data`: optional object (event-specific payload).

## Event kinds

Currently defined event kinds:
- `lease_created`
- `lease_released`
- `workspace_created`
- `workspace_removed`
- `commit_blocked`
- `commit_created`

## Payloads

Payloads are event-specific and may be omitted. The first implemented payload
is for `lease_created`, emitted by `sv take`:

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

If the actor is not set, `actor` is omitted in the envelope and the payload
actor is `null`.

`sv release` emits `lease_released` with this payload:

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
