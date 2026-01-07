---
id: sv-8jf.1.7
status: closed
deps: []
links: []
created: 2025-12-31T14:36:47.892271531+01:00
type: task
priority: 1
parent: sv-8jf.1
---
# Actor system: SV_ACTOR, --actor, sv actor set

Implement actor identity:
- Read from SV_ACTOR env var
- Override via --actor flag
- sv actor set <name> command to persist to .sv/
- Actor used for lease ownership and op log attribution
- Fallback to 'unknown' per config

Acceptance: sv correctly identifies actor from all sources, persists setting


