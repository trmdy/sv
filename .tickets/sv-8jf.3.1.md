---
id: sv-8jf.3.1
status: closed
deps: []
links: []
created: 2025-12-31T14:37:26.863512092+01:00
type: task
priority: 0
parent: sv-8jf.3
---
# Lease data model: schema and storage format

Design and implement lease data model:
- Fields: id (uuid), pathspec, strength (observe|cooperative|strong|exclusive), intent (bugfix|feature|docs|refactor|rename|format|mechanical|investigation|other), actor, scope (repo|branch:<name>|ws:<workspace>), note, ttl, expires_at, hints (optional: symbols, lines)
- JSONL storage format in .git/sv/leases.jsonl
- Serde serialization/deserialization
- Validation rules

Per spec Section 3.4

Acceptance: lease struct serializes correctly, validation catches invalid data


