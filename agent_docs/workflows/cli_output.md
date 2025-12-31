# CLI Output Contract

This document defines the sv CLI output contract for human-readable output,
JSON output, and error/exit semantics. It is the source of truth for any
command that prints to stdout/stderr.

## Goals

- Stable, predictable output for automation and scripting.
- Human output that is concise, actionable, and consistent across commands.
- Single-point error emission with reliable exit codes.
- Short, plain language with consistent terminology (use "workspace"); avoid emoji.

## Output modes

### Human mode (default)

- Intended for terminals and humans.
- Output is structured with a one-line header followed by zero or more
  sections (Summary, Details, Warnings, Next steps).
- Only print sections that have content.
- Warnings and errors never appear in success-only paths.
- Tone: short sentences, no emoji, no marketing language.

### JSON mode (`--json`)

- Output is a single JSON object on stdout.
- No other text output (no headers, no warnings, no progress).
- Errors are emitted as a single JSON error object (see below).

### Quiet mode (`--quiet`)

- Suppress non-essential output on success.
- Errors still print (human mode: stderr; JSON mode: stdout error object).

## Human output layout

### Header

The first line is always a short summary:

```
sv <command>: <short outcome>
```

Examples:

```
sv init: initialized repo
sv take: created 3 leases
sv protect status: 4 rules (1 disabled)
```

### Sections

Sections are titled with a single word and a colon, followed by bullets:

```
Summary:
- key: value
- key: value

Details:
- ...

Warnings:
- ...

Next steps:
- command or action
```

Section rules:

- Use `Summary` for the most important facts a user needs immediately.
- Use `Details` for supporting information.
- Use `Warnings` for non-fatal issues or degraded behavior.
- Use `Next steps` for actionable commands or guidance.
- Omit empty sections.

## Error handling and exit codes

Errors are emitted once and mapped to these exit codes:

- `0`: success
- `2`: user error (invalid args, missing repo)
- `3`: policy blocked (protected paths, lease conflicts)
- `4`: operation failed (git errors, merge conflicts)

### Human error format

- Errors are printed to stderr as a single line:

```
error: <message>
```

- If action is required, append a short hint on the next line:

```
error: protected path would be committed: .beads/tasks.jsonl
hint: use `sv protect off .beads/**` or `--allow-protected` if intentional
```

### JSON error format

Errors are emitted as a single JSON object on stdout with a consistent envelope:

```json
{
  "schema_version": "sv.v1",
  "command": "protect status",
  "status": "error",
  "error": {
    "message": "protected path would be committed: .beads/tasks.jsonl",
    "code": 3,
    "kind": "policy_blocked",
    "details": {
      "path": ".beads/tasks.jsonl",
      "rule": ".beads/**"
    }
  },
  "warnings": [],
  "next_steps": ["sv protect off .beads/**", "sv commit --allow-protected"]
}
```

`kind` values map to exit codes:

- `user_error` -> 2
- `policy_blocked` -> 3
- `operation_failed` -> 4

`details` is optional and may include command-specific fields.

## JSON schema envelope

All successful JSON output must use the same top-level envelope:

```json
{
  "schema_version": "sv.v1",
  "command": "<command>",
  "status": "success",
  "data": { /* command-specific */ },
  "warnings": [ /* optional */ ],
  "next_steps": [ /* optional */ ]
}
```

Rules:

- `schema_version` is always present.
- `command` is the fully-qualified command name (e.g., `take`, `protect status`).
- `status` is `success` or `error`.
- `data` or `error` is required depending on `status`.
- `warnings` and `next_steps` are optional arrays of strings.

### Versioning policy

- `sv.v1` is the initial schema version for all JSON output.
- Adding new optional fields is backward compatible.
- Removing or renaming fields, or changing field types, requires a new major
  version (e.g., `sv.v2`).

## Examples

### `sv init`

Human:

```
sv init: initialized repo
Summary:
- repo: /path/to/repo
- created: .sv.toml, .sv/, .git/sv/
- updated: .gitignore

Next steps:
- sv actor set <name>
- sv ws new <workspace>
```

JSON:

```json
{
  "schema_version": "sv.v1",
  "command": "init",
  "status": "success",
  "data": {
    "repo": "/path/to/repo",
    "created": {
      "config": true,
      "sv_dir": true,
      "git_sv": true
    },
    "updated": {
      "gitignore": true
    }
  },
  "warnings": [],
  "next_steps": ["sv actor set <name>", "sv ws new <workspace>"]
}
```

### `sv take`

Human:

```
sv take: created 2 leases (1 conflict)
Summary:
- actor: alice
- leases_created: 2
- conflicts: 1

Details:
- src/auth/** (cooperative, intent: bugfix, ttl: 2h)
- src/session.rs (strong, intent: refactor, ttl: 2h)

Warnings:
- conflict: src/auth/token.rs held by bob (exclusive)

Next steps:
- sv lease who src/auth/token.rs
- retry with --allow-overlap if intentional
```

JSON:

```json
{
  "schema_version": "sv.v1",
  "command": "take",
  "status": "success",
  "data": {
    "actor": "alice",
    "created": [
      {
        "id": "lease_123",
        "path": "src/auth/**",
        "strength": "cooperative",
        "intent": "bugfix",
        "ttl": "2h"
      },
      {
        "id": "lease_124",
        "path": "src/session.rs",
        "strength": "strong",
        "intent": "refactor",
        "ttl": "2h"
      }
    ],
    "conflicts": [
      {
        "path": "src/auth/token.rs",
        "holder": "bob",
        "strength": "exclusive"
      }
    ],
    "summary": {
      "created": 2,
      "conflicts": 1
    }
  },
  "warnings": ["conflict: src/auth/token.rs held by bob (exclusive)"],
  "next_steps": ["sv lease who src/auth/token.rs", "retry with --allow-overlap if intentional"]
}
```

### `sv protect status`

Human:

```
sv protect status: 4 rules (1 disabled)
Summary:
- rules: 4
- disabled_for_workspace: 1

Details:
- .beads/** (guard)
- Cargo.lock (warn)
- docs/** (readonly)
- .github/** (guard) [disabled]

Warnings:
- staged files match protected patterns: Cargo.lock

Next steps:
- sv protect off .github/**
- sv protect rm <pattern>
```

JSON:

```json
{
  "schema_version": "sv.v1",
  "command": "protect status",
  "status": "success",
  "data": {
    "rules": [
      {"pattern": ".beads/**", "mode": "guard", "disabled": false},
      {"pattern": "Cargo.lock", "mode": "warn", "disabled": false},
      {"pattern": "docs/**", "mode": "readonly", "disabled": false},
      {"pattern": ".github/**", "mode": "guard", "disabled": true}
    ],
    "matches": {
      "staged": ["Cargo.lock"],
      "disabled": [".github/**"]
    }
  },
  "warnings": ["staged files match protected patterns: Cargo.lock"],
  "next_steps": ["sv protect off .github/**", "sv protect rm <pattern>"]
}
```

### `sv status`

Human:

```
sv status: workspace ready
Summary:
- actor: alice
- workspace: agent1 (/path/to/repo/.sv/ws/agent1)
- base: origin/main

Details:
- active leases: 2
- protected overrides: 1

Warnings:
- expired leases detected: 1

Next steps:
- sv lease ls --actor alice
- sv protect status
```

JSON:

```json
{
  "schema_version": "sv.v1",
  "command": "status",
  "status": "success",
  "data": {
    "actor": "alice",
    "workspace": {
      "name": "agent1",
      "path": "/path/to/repo/.sv/ws/agent1",
      "base": "origin/main"
    },
    "leases": {
      "active": 2,
      "expired": 1
    },
    "protect_overrides": 1
  },
  "warnings": ["expired leases detected"],
  "next_steps": ["sv lease ls --actor alice", "sv protect status"]
}
```
