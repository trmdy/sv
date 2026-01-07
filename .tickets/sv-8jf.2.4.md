---
id: sv-8jf.2.4
status: closed
deps: []
links: []
created: 2025-12-31T14:37:10.081126702+01:00
type: task
priority: 1
parent: sv-8jf.2
---
# sv ws info: detailed workspace information

Implement sv ws info <name>:
- Show detailed workspace info:
  - name, path, branch, base ref
  - touched paths (files changed vs base)
  - leases affecting this workspace
  - ahead/behind counts vs base and main
  - recent Change-Ids (if any)
  - actor assignment
  - last activity timestamp

Acceptance: sv ws info agent1 shows comprehensive details


