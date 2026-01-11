---
id: sv-8jf.1.14
status: closed
deps: [sv-8jf.1.6]
links: []
created: 2025-12-31T14:45:52.080641113+01:00
type: task
priority: 1
parent: sv-8jf.1
---
# Git commit and trailer manipulation

Implement commit operations:
- Create commit with message
- Amend commit message
- Parse commit trailers (Change-Id)
- Inject/modify trailers
- Support for -m, -F message sources

Acceptance: can create commits with trailers, parse existing trailers


