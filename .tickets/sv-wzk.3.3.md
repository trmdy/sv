---
id: sv-wzk.3.3
status: closed
deps: [sv-wzk.3.6]
links: []
created: 2025-12-31T14:40:02.944662936+01:00
type: task
priority: 1
parent: sv-wzk.3
---
# Change-Id deduplication logic

Implement Change-Id deduplication for hoist:
- Parse Change-Id trailers from commits
- Group commits by Change-Id
- Compare patch-ids for duplicates
- If identical patch-id: include once
- If diverged: require resolution (--prefer flag or config)
- Emit warnings for diverged Change-Ids

Acceptance: hoist correctly deduplicates same logical changes


