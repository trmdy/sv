---
id: sv-8jf.1.10
status: closed
deps: []
links: []
created: 2025-12-31T14:45:20.028025646+01:00
type: task
priority: 2
parent: sv-8jf.1
---
# Event output for external integrations

Implement optional event output stream:
- --events flag to emit structured events to stdout/file
- Events: lease_created, lease_released, workspace_created, commit_blocked, etc.
- JSON format for machine consumption
- Enables integration with MCP mail, Slack, etc. without hard dependency
- Document event schema

Per spec summary: 'optional event output so other systems can integrate'

Acceptance: sv take --events emits JSON event for lease creation


