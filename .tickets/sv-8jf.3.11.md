---
id: sv-8jf.3.11
status: closed
deps: []
links: []
created: 2025-12-31T14:45:11.19864062+01:00
type: task
priority: 1
parent: sv-8jf.3
---
# Lease note requirement validation

Enforce --note requirement for strong/exclusive leases:
- strong strength: require --note flag
- exclusive strength: require --note flag
- cooperative/observe: --note optional
- Clear error message if note missing
- Configurable in .sv.toml (can disable requirement)

Per spec Section 3.4: 'note (required for strong|exclusive, optional otherwise)'

Acceptance: sv take --strength exclusive fails without --note


