---
id: sv-2pg.2
status: closed
deps: [sv-2pg.1]
links: []
created: 2025-12-31T15:23:30.361278165+01:00
type: task
priority: 1
parent: sv-2pg
---
# Output formatting helper module

Implement a reusable output helper (e.g., src/output.rs) to centralize human + JSON formatting.

Requirements:
- Accept a structured payload and render in human or JSON modes.
- Standard sections: header, details, warnings, next steps.
- Allow command-specific fields without breaking schema contract.
- Respect quiet mode (no human output) and JSON mode (no extra noise).

Why: Consistency + lower cognitive load makes the CLI feel premium and predictable.


## Acceptance Criteria

Shared helper used by at least sv init/take/status; JSON and human output go through one path; quiet mode suppresses human output; JSON output matches schema from sv-2pg.1.


