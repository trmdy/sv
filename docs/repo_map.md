# Repo map

## Top level
- `AGENTS.md`: multi-agent operating manual and coordination rules.
- `PRODUCT_SPECIFICATION.md`: product spec for sv CLI (MVP scope, workflows, and roadmap).
- `Cargo.toml`: Rust crate manifest; `sv` binary; library declared as `src/lib.rs` (currently missing).
- `src/`: CLI entry point and library modules (currently `main.rs` only).
- `agent_docs/`: runbooks, workflows, gotchas, and decisions for agents.
- `.beads/`: task tracker state (use `bd`; do not hand-edit).
- `.tasks/`: sv task manager state (tracked).

## agent_docs/
- `agent_docs/README.md`: index and required reading order.
- `agent_docs/runbooks/`: dev/test/release runbooks (templates).
- `agent_docs/workflows/`: coordination and commit-pass workflows.
- `agent_docs/gotchas.md`: pitfalls list (currently template).
- `agent_docs/decisions/`: ADR-like decisions (template present).
