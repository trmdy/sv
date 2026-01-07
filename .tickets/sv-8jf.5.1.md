---
id: sv-8jf.5.1
status: closed
deps: []
links: []
created: 2025-12-31T14:38:25.396981761+01:00
type: task
priority: 0
parent: sv-8jf.5
---
# sv commit: basic git commit wrapper

Implement sv commit [sv-flags...] -- [git commit args...]:
- Pass through to git commit
- Support -m, -F, --amend, -a, --no-edit (v0.1 focus)
- Determine set of paths to be committed (git diff --cached --name-only)
- Handle -a flag (stage all modified)
- Execute git commit with final message

Acceptance: sv commit -m 'msg' works like git commit -m 'msg'


