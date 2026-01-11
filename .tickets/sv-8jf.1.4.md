---
id: sv-8jf.1.4
status: closed
deps: []
links: []
created: 2025-12-31T14:36:37.793791256+01:00
type: task
priority: 1
parent: sv-8jf.1
---
# Storage layer: .sv/ and .git/sv/ directory structure

Implement storage layer:
- .sv/ for workspace-local state (ignored)
- .git/sv/ for shared local state per clone (ignored)
  - workspaces.json registry
  - leases.jsonl (or sqlite later)
  - oplog/ directory
- Ensure directories created atomically
- Git ignore entries if needed

Acceptance: sv init creates correct structure, directories persist across commands


