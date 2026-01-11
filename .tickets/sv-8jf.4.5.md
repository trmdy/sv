---
id: sv-8jf.4.5
status: closed
deps: []
links: []
created: 2025-12-31T14:38:15.120891899+01:00
type: task
priority: 0
parent: sv-8jf.4
---
# Protected path enforcement during commit

Integrate protected path checking into sv commit:
- Detect if staged files match any protected patterns
- guard mode: block commit with clear error (exit 3)
- readonly mode: (future) prevent file modification entirely
- warn mode: emit warning but allow commit
- Check per-workspace overrides
- Support --allow-protected flag to override

Acceptance: sv commit blocks protected files unless override used


