---
id: sv-wzk.4.1
status: closed
deps: []
links: []
created: 2025-12-31T14:40:05.514915932+01:00
type: task
priority: 1
parent: sv-wzk.4
---
# Selector language: parser and AST

Implement selector language parser:
- Grammar for entities: ws(...), lease(...), branch(...)
- Predicates: active, stale, name~'regex', ahead('ref'), touching('pathspec'), blocked
- Operators: | (union), & (intersection), ~ (difference), parentheses
- Parse to AST representation

Per spec Section 12

Acceptance: parse 'ws(active) & ahead("main")' into AST


