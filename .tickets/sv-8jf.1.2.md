---
id: sv-8jf.1.2
status: closed
deps: []
links: []
created: 2025-12-31T14:36:30.421595563+01:00
type: task
priority: 0
parent: sv-8jf.1
---
# CLI framework: clap setup with global flags

Implement CLI framework using clap:
- Global flags: --repo, --actor, --json, --quiet, --verbose
- Subcommand structure for: ws, take, release, lease, protect, commit, risk, op
- Exit code constants (0, 2, 3, 4 per spec)
- JSON output mode infrastructure

Acceptance: sv --help shows all subcommand stubs, global flags work


