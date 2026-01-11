---
id: sv-8jf.1.3
status: closed
deps: []
links: []
created: 2025-12-31T14:36:33.945807721+01:00
type: task
priority: 0
parent: sv-8jf.1
---
# Configuration system: .sv.toml parsing and defaults

Implement configuration loading:
- Parse .sv.toml from repo root
- Default values (base=main, default_strength=cooperative, default_ttl=2h, etc.)
- Config struct with serde
- Validation and error messages
- Config lookup from any subdirectory

Schema per Appendix A of PRODUCT_SPECIFICATION.md

Acceptance: sv reads .sv.toml, applies defaults, reports errors for invalid config


