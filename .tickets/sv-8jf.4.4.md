---
id: sv-8jf.4.4
status: closed
deps: []
links: []
created: 2025-12-31T14:38:11.817323007+01:00
type: task
priority: 1
parent: sv-8jf.4
---
# sv protect off: disable protection per-workspace

Implement sv protect off <pattern...> [--workspace]:
- Disable protection for specified patterns in current workspace only
- Store override in .sv/ (local, ignored)
- Does not modify .sv.toml
- Useful for designated 'lockfile updater' workspace

Acceptance: sv protect off pnpm-lock.yaml allows commits in this workspace only


