---
id: sv-8jf.8.4
status: closed
deps: []
links: []
created: 2025-12-31T14:40:32.927603464+01:00
type: task
priority: 1
parent: sv-8jf.8
---
# Concurrency stress tests

Multi-process concurrency tests:
- Parallel lease creation/release
- Concurrent workspace registry updates
- Oplog append under contention
- File lock timeout behavior
- No data corruption under stress

Critical per spec Section 13.4


