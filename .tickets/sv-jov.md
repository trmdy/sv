---
id: sv-jov
status: open
deps: [sv-844]
links: []
created: 2026-01-01T11:35:06.060117592+01:00
type: feature
priority: 2
---
# Add 'sv resolve' command to mark conflicts as resolved

CLI command to mark in-conflict commits as resolved after user fixes conflict markers.

## Command Interface
```bash
sv resolve <commit-id>           # Mark specific commit as resolved
sv resolve --all                 # Mark all resolved (auto-detect)
sv resolve --check <commit-id>   # Check if commit still has markers
```

## Behavior
1. Verify the commit's files no longer contain conflict markers
2. Update `.git/sv/conflicts.jsonl` with `resolved_at` timestamp
3. Optionally create a new commit if working tree has changes

## Auto-detection
Could also auto-detect resolution during:
- `sv status`
- `sv commit`
- `sv hoist`

## Depends on
- sv-844 (conflict tracking infrastructure)


