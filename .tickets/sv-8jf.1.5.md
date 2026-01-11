---
id: sv-8jf.1.5
status: closed
deps: []
links: []
created: 2025-12-31T14:36:41.368987822+01:00
type: task
priority: 1
parent: sv-8jf.1
---
# Concurrency: file locking primitives for .git/sv/

Implement concurrency-safe file operations:
- File locking (flock or equivalent) for .git/sv/ writes
- Atomic write pattern (write temp + rename)
- Lock timeout with configurable wait
- Error handling for lock contention

Critical for multi-agent safety per spec Section 13.4

Acceptance: concurrent sv processes don't corrupt state, locks timeout gracefully


