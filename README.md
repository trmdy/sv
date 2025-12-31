# sv — Simultaneous Versioning

Git coordination for parallel agents. sv adds workspaces (worktrees), graded
leases, protected paths, and early conflict detection so large teams can move
fast without stepping on each other.

Status: early development. The CLI surface is in flux; run `sv --help` to see
what is currently implemented.

## Why sv?

Multi-agent coding in a shared repo fails for predictable reasons:

- Agents edit the same files without realizing it.
- Conflicts show up late, after lots of work.
- "Who owns this area?" is unclear.
- Automation mistakes are hard to undo.

sv adds coordination primitives while staying Git-native.

## What you get

- Workspaces: first-class Git worktrees per agent.
- Leases: graded reservations with intent and TTL.
- Protected paths: guardrails for global files like `.beads/**`.
- Risk checks: overlap detection before conflicts hit.
- Change-Ids: stable identifiers for continuous integration of work.
- Operation log + undo: safe automation with reversibility.

## Quickstart (5-minute workflow)

Prereqs: Rust 1.70+ and Git.

1) Build from source:

```bash
cargo build --release
```

2) Initialize sv in a repo:

```bash
./target/release/sv init
```

3) Set your actor identity:

```bash
./target/release/sv actor set alice
```

4) Create a workspace and take a lease:

```bash
./target/release/sv ws new agent1
./target/release/sv take src/auth/** --strength cooperative --intent bugfix --note "Fix refresh edge case"
```

5) Commit with checks and preview risk:

```bash
./target/release/sv commit -m "Fix refresh edge case"
./target/release/sv risk
```

If a command is missing or behaves differently, check the current CLI surface:

```bash
./target/release/sv --help
```

## Configuration

sv reads `.sv.toml` from the repo root when present and falls back to defaults.
See `PRODUCT_SPECIFICATION.md` for the full schema and examples.

## Documentation

- Product spec: `PRODUCT_SPECIFICATION.md`
- Developer docs: `agent_docs/README.md`
- Runbooks: `agent_docs/runbooks/`

## Contributing

This repo uses Beads for task tracking and MCP Agent Mail for coordination.
See `AGENTS.md` before starting work. Tests and dev workflow are documented in
`agent_docs/runbooks/`.
