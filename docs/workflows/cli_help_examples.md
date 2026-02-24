# CLI Help Examples Alignment

Goal: keep `sv <cmd> --help` examples consistent with the output contract in
`agent_docs/workflows/cli_output.md`, without duplicating full output payloads.

## Principles

- Examples should be command lines only; do not embed full output in help text.
- Prefer short, realistic workflows over one-off commands.
- Always show flag usage for `--json` and `--events` in at least one example.
- Avoid ambiguous stdout mixing: show `--events <path>` when `--json` is used.
- Keep terminology consistent: use "workspace" (not worktree) in text.

## Recommended flows to include

### Bootstrap + lease + commit

```bash
sv init
sv actor set alice
sv ws new agent1
sv take src/auth/** --strength cooperative --intent bugfix --note "Fix refresh edge case"
sv commit -m "Fix refresh edge case"
```

### JSON output + events stream

```bash
sv take src/auth/** --json --events /tmp/sv.events.jsonl
```

### Protected path or lease conflict guidance

```bash
sv protect status
sv commit --allow-protected
sv lease who src/auth/token.rs
sv commit --force-lease
```

### Workspace overview + risk

```bash
sv ws list
sv ws info agent1
sv switch agent1
cd "$(sv switch agent1)"
sv risk
```

## Command-specific example hints

- `sv take`: show `--note` when using strong/exclusive strengths.
- `sv release`: include a pathspec and a lease id variant (short id).
- `sv protect`: show `add`, `status`, and `rm` for a single pattern.
- `sv commit`: include `-m`, `--amend`, `--no-edit` in separate examples.
- `sv ws new`: show `--base` and `--dir` once, not in every help section.
