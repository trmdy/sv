---
id: sv-wzk.2.1
status: closed
deps: []
links: []
created: 2025-12-31T14:39:52.051185779+01:00
type: task
priority: 1
parent: sv-wzk.2
---
# sv onto: basic workspace rebase

Implement sv onto <target-workspace> [--strategy rebase|merge|cherry-pick] [--base <ref>]:
- Reposition current workspace branch on top of target's tip
- Default strategy: rebase (linear history)
- Handle conflicts with clear guidance
- Record operation in oplog for undo

Acceptance: sv onto agent5 rebases current workspace onto agent5's branch


