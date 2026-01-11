---
id: sv-8jf.2.6
status: closed
deps: []
links: []
created: 2025-12-31T14:37:15.228716162+01:00
type: task
priority: 1
parent: sv-8jf.2
---
# Workspace registry: JSON schema and CRUD operations

Implement workspace registry (.git/sv/workspaces.json):
- Schema: id, name, path, branch, base, actor, created_at, last_active
- CRUD operations with file locking
- Validation (no duplicate names, paths exist)
- Cleanup of stale entries (path doesn't exist)
- Atomic updates

Acceptance: registry survives concurrent access, validates entries


