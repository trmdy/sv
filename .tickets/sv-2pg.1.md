---
id: sv-2pg.1
status: closed
deps: []
links: []
created: 2025-12-31T15:23:01.822241172+01:00
type: task
priority: 1
parent: sv-2pg
---
# CLI output style guide + JSON schema contract

Create a self-contained doc that defines the human output format and JSON schema contract for sv commands. This is the single source of truth for UX consistency.

Include:
- Output layout (header, bullet sections, warnings, next steps) with tone guidance (short, plain language, no emoji).
- Error format for human + JSON (fields + exit code mapping).
- JSON schema versioning and backwards-compatibility policy.
- Rules for quiet mode and JSON mode (no extra noise).
- Standard JSON envelope fields (e.g., schema_version, command, status, data, warnings, next_steps).
- Examples for `sv init`, `sv take`, `sv protect status`, `sv status`.

Suggested location: `agent_docs/workflows/cli_output.md`.

Rationale: A stable output contract makes the CLI feel premium and is critical for automation and future integrations.


## Acceptance Criteria

Doc added with concrete examples and versioned JSON schema; approved by team; referenced by CLI output helpers.


