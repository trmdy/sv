---
id: sv-8jf.1.8
status: closed
deps: []
links: []
created: 2025-12-31T14:36:50.94532698+01:00
type: task
priority: 1
parent: sv-8jf.1
---
# Error handling: structured errors with exit codes

Implement error handling:
- Error types for user error (2), policy block (3), operation failed (4)
- Structured error messages (human + JSON modes)
- Context propagation (which file, which lease, etc.)
- Error chaining for debugging

Acceptance: errors are clear, exit codes match spec, --json gives structured errors


