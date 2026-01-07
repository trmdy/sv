---
id: sv-mor
status: closed
deps: [sv-844]
links: []
created: 2026-01-01T11:31:44.921173572+01:00
type: feature
priority: 2
---
# Add jj-style conflict propagation: conflicts should not abort hoist but propagate to commits marked as in-conflict until resolved

Inspired by jujutsu (jj), implement conflict propagation instead of conflict abortion.

## Current Behavior
When sv hoist encounters a merge conflict, it either:
- Stops immediately (default)
- Continues but skips conflicting commits (--continue-on-conflict)

## Desired Behavior (jj-style)
- Conflicts should NOT cause aborts
- Conflicting state should be committed as-is with conflict markers
- The commit should be marked as 'in-conflict' status
- Subsequent commits can be applied on top of the conflicting commit
- The conflict propagates until manually resolved
- sv status should show which commits are in-conflict state

## Benefits
- No work is lost when conflicts occur
- Allows seeing the full integration result even with conflicts
- Conflicts can be resolved later without re-running hoist
- Better matches jj's conflict-as-data philosophy

## Implementation Notes
- Add new HoistCommitStatus::InConflict variant
- When cherry-pick conflicts, write the conflicting index to a tree and commit it
- Track conflict markers in the commit (possibly in trailer or separate metadata)
- sv status should warn about in-conflict commits
- Consider adding 'sv resolve' command to mark conflicts as resolved


