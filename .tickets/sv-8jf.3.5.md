---
id: sv-8jf.3.5
status: closed
deps: []
links: []
created: 2025-12-31T14:37:39.904847048+01:00
type: task
priority: 1
parent: sv-8jf.3
---
# sv lease who: show lease holders for path

Implement sv lease who <path>:
- Find all leases that overlap with the given path
- Show holder info: actor, strength, intent, note, expires_at
- Handle glob matching (both directions: lease glob matches path, path matches lease glob)
- Useful for 'who is working on this file?'

Acceptance: sv lease who src/auth/login.rs shows overlapping leases


