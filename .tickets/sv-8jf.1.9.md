---
id: sv-8jf.1.9
status: closed
deps: []
links: []
created: 2025-12-31T14:45:03.439770786+01:00
type: task
priority: 1
parent: sv-8jf.1
---
# sv init: initialize sv in a repository

Implement sv init command:
- Create .sv.toml with default config if not exists
- Create .git/sv/ directory structure
- Add .sv/ to .gitignore if not present
- Validate this is a Git repository
- Idempotent: safe to run multiple times

Acceptance: sv init in a git repo creates config and storage dirs


