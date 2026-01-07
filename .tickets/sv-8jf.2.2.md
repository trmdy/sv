---
id: sv-8jf.2.2
status: closed
deps: []
links: []
created: 2025-12-31T14:37:05.496873944+01:00
type: task
priority: 1
parent: sv-8jf.2
---
# sv ws here: register current directory as workspace

Implement sv ws here [--name <name>]:
- Register current directory as workspace (for single-checkout usage)
- Auto-derive name from directory or branch if not specified
- Create .sv/ local state directory
- Add to .git/sv/workspaces.json
- Validate it's a valid Git checkout

Acceptance: sv ws here in main checkout registers it, sv ws list shows it


