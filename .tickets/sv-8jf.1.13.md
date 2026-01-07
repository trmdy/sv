---
id: sv-8jf.1.13
status: closed
deps: [sv-8jf.1.6]
links: []
created: 2025-12-31T14:45:52.041218639+01:00
type: task
priority: 1
parent: sv-8jf.1
---
# Git diff operations for touched files

Implement diff operations:
- Get changed files between refs (git diff --name-only)
- Get staged files list
- Detect file status (added/modified/deleted)
- Support pathspec filtering

Needed for: sv risk overlap detection, sv commit file list

Acceptance: can enumerate touched/staged files accurately


