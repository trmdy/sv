---
id: sv-8jf.4.6
status: closed
deps: []
links: []
created: 2025-12-31T14:45:05.24735692+01:00
type: task
priority: 2
parent: sv-8jf.4
---
# sv protect rm: remove protected patterns

Implement sv protect rm <pattern...>:
- Remove patterns from .sv.toml [protect] section
- Match exact pattern strings
- Error if pattern not found (unless --force)
- Update file atomically

Acceptance: sv protect rm 'generated/**' removes pattern from config


