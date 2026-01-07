---
id: sv-8jf.4.1
status: closed
deps: []
links: []
created: 2025-12-31T14:38:03.660440622+01:00
type: task
priority: 0
parent: sv-8jf.4
---
# Protected paths configuration in .sv.toml

Implement protected paths configuration:
- [protect] section in .sv.toml
- mode = 'guard' | 'readonly' | 'warn' (default: guard)
- paths = ['.beads/**', 'pnpm-lock.yaml', 'Cargo.lock', ...]
- Glob pattern support
- Per-path mode override

Per spec Section 7 and Appendix A

Acceptance: .sv.toml protect section parsed, patterns validated


