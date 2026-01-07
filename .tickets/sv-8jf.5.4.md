---
id: sv-8jf.5.4
status: closed
deps: []
links: []
created: 2025-12-31T14:38:35.987334842+01:00
type: task
priority: 1
parent: sv-8jf.5
---
# Commit operation logging

Record commit operations in oplog:
- Log: commit hash, actor, timestamp, files changed, Change-Id
- Record before/after ref state (for potential undo)
- Include any policy overrides used (--allow-protected, --force-lease)

Acceptance: sv op log shows commit history with details


