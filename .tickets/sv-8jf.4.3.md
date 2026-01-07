---
id: sv-8jf.4.3
status: closed
deps: []
links: []
created: 2025-12-31T14:38:08.748854169+01:00
type: task
priority: 1
parent: sv-8jf.4
---
# sv protect add: add protected patterns

Implement sv protect add <pattern...> [--mode guard|readonly|warn]:
- Add patterns to .sv.toml [protect] section
- Validate pattern syntax
- Merge with existing patterns (no duplicates)
- Default mode from config or guard

Acceptance: sv protect add 'generated/**' updates .sv.toml


