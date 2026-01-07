---
id: sv-8jf.6.2
status: closed
deps: []
links: []
created: 2025-12-31T14:38:48.523475075+01:00
type: task
priority: 1
parent: sv-8jf.6
---
# Risk scoring with lease metadata

Enhance sv risk with lease-aware scoring:
- Factor in lease strength (exclusive/strong overlaps are higher risk)
- Factor in intent (format/rename more likely to conflict)
- Score overlaps: low/medium/high/critical
- Highlight 'hot' overlaps requiring attention

Per spec Section 9.1

Acceptance: sv risk shows severity scores based on lease metadata


