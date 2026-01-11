---
id: sv-wzk.4.2
status: closed
deps: []
links: []
created: 2025-12-31T14:40:07.802900908+01:00
type: task
priority: 1
parent: sv-wzk.4
---
# Selector language: evaluation engine

Implement selector evaluation:
- Evaluate predicates against workspace/lease/branch data
- Execute set operations (union, intersection, difference)
- Return matching entities
- Integrate with sv ws list -s, sv risk -s, sv hoist -s, sv lease ls -s

Acceptance: selectors filter entities correctly across all commands


