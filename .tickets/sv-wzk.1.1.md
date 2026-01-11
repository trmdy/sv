---
id: sv-wzk.1.1
status: closed
deps: []
links: []
created: 2025-12-31T14:39:47.190423584+01:00
type: task
priority: 1
parent: sv-wzk.1
---
# Virtual merge infrastructure using libgit2

Implement virtual merge capability:
- Create in-memory merge without touching working tree
- Use libgit2's merge analysis and merge trees
- Detect conflicts: content, add-add, modify-delete, rename
- Return conflict report with file paths and types

Acceptance: can simulate merge of two branches and detect conflicts


