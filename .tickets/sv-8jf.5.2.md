---
id: sv-8jf.5.2
status: closed
deps: []
links: []
created: 2025-12-31T14:38:29.187218078+01:00
type: task
priority: 0
parent: sv-8jf.5
---
# Change-Id injection in commit messages

Implement Change-Id trailer injection:
- Generate stable UUID for Change-Id
- Add 'Change-Id: <uuid>' trailer if missing
- Preserve existing Change-Id on --amend
- Handle -m flag (append to message)
- Handle -F flag (append to file content)
- Format per Git trailer conventions

JJ-inspired per spec Section 3.6

Acceptance: sv commit -m 'msg' produces commit with Change-Id trailer


