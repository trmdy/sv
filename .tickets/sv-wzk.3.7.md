---
id: sv-wzk.3.7
status: closed
deps: [sv-wzk.3.6]
links: []
created: 2025-12-31T14:46:17.46042713+01:00
type: task
priority: 1
parent: sv-wzk.3
---
# Hoist cherry-pick replay engine

Implement commit replay for hoist:
- Cherry-pick commits in order onto integration branch
- Handle conflicts (fail or continue based on flag)
- Track progress for resumption
- Report success/failure per commit
- Preserve Change-Id trailers

Acceptance: sv hoist replays commits correctly, handles conflicts


