# AGENTS.md — Project Agent Operating Manual

This repo is commonly worked on by MULTIPLE AGENTS IN PARALLEL, often in the SAME working directory.
Assume the workspace may change while you work.

If anything is unclear or conflicts with observed reality, STOP and ask rather than guessing.

---

## 0) Repo Quick Facts (EDIT PER REPO)

- Project: <name>
- Primary stack: <e.g. bun / node / python / go / rust>
- “How to run” (authoritative): `agent_docs/runbooks/dev.md`
- “How to test” (authoritative): `agent_docs/runbooks/test.md`
- “How to release/deploy” (if applicable): `agent_docs/runbooks/release.md`
- Repo map / key directories: `agent_docs/repo_map.md`
- Known pitfalls: `agent_docs/gotchas.md`

---

## 1) Non‑negotiables (read first)

### Multi-agent coordination is mandatory

- Before editing files, coordinate via **MCP Agent Mail**.
- Reserve the paths you will touch (leases) and announce intent.
- Only edit files you have reserved (or have explicit permission to share).

### No destructive actions without explicit approval

Do NOT run (or propose as a “quick fix”) without explicit user approval:
- `git reset --hard`
- `git clean -fd` (or variants)
- `rm -rf` (or any delete/overwrite command with broad scope)
- anything that deletes data, generated artifacts, or repo history

When in doubt: ask first.

### Keep diffs scoped

- No drive-by refactors.
- No mass reformatting.
- No “cleanup” outside the leased scope.

### Beads hygiene

- Close Beads tasks immediately when done; do not leave completed work in `in_progress`.

---

## 2) Fast Start Checklist (do this at the beginning of a task)

1) Read the index: `agent_docs/README.md`
2) Check MCP Agent Mail:
   - read inbox / recent thread activity
   - see if leases already exist on your target paths
3) Pick/confirm the task source (Beads is default):
   - `bd ready --json`
4) Announce intent (Agent Mail):
   - task id (if any), goal, target paths, expected outputs
5) Reserve files BEFORE editing (Agent Mail leases):
   - reserve the specific files/dirs you’ll change
6) Pull relevant memory for non-trivial work:
   - `cm context "<what you are about to do>" --json`
7) Before implementing something that might have been solved already:
   - `cass search "<keywords>" --robot --limit 5`

---

## 3) Coordination Protocol (MCP Agent Mail)

Use Agent Mail for:
- work announcements + status updates
- file reservations (leases)
- handoffs (“what changed / what to test / what’s next”)

Rules:
- Acquire leases before editing.
- If you hit a conflict, do not brute-force. Coordinate (adjust scope, wait, or get permission).
- Release leases when done.
- Post a final handoff message at the end of your work session.

If MCP Agent Mail tools are not available in your harness/runtime, tell the user immediately.

---

## 4) Task System (Beads is default)

Beads (`bd`) is the canonical task tracker.

- Quickstart to get a full overview over bd functionality: `bd quickstart`.
- Find ready work: `bd ready --json`
- Claim work: `bd update <id> --status in_progress --json`
- Create follow-ups: `bd create "Title" -t task -p 2 --json`
- Close work: `bd close <id> --reason "…" --json` (must close beads immediately when done; do not leave completed work in `in_progress`)

Project state:
- `.beads/` is authoritative and should be committed alongside related code changes.
- Do not hand-edit beads JSONL; use `bd`.

---

## 5) Quality Gate (Definition of Done)

Before you call work “done” or hand off:

1) Run the repo’s test/build/lint gates:
   - Follow `agent_docs/runbooks/test.md`
2) Run UBS bug scan (scope to changed files when possible):
   - preferred: `ubs --staged` (if staging is in use)
   - otherwise: `ubs --diff` (or `ubs .` for full scan)
3) Summarize:
   - what changed
   - how verified (exact commands)
   - risks / follow-ups

---

## 6) Git & PR Workflow (multi-agent safe)

### Commit messages
- Use a simple descriptive title. No strict column limits.
- Use a richer body explaining in detail what the commit does and why it exists.

### Default workflow: “Commit pass” (current)
In many projects, agents do not commit continuously.
Instead, we group changes into logical commits in a commit pass.

If you are NOT the integrator for this task:
- avoid committing/pushing unless explicitly requested
- leave a clean handoff summary + suggested commit grouping
- do not stage/commit unrelated changes

### Optional workflow: Continuous commits (allowed when explicitly chosen)
If the team chooses continuous commits for a task:
- keep commits small and coherent
- do not commit broken states unless explicitly agreed
- always stage explicitly (see below)

### Critical staging rule in shared working dirs
- NEVER use `git add -A` in a multi-agent shared working directory.
- Always stage explicit paths:
  - `git add path/to/file1 path/to/file2`
- Always verify:
  - `git diff --cached`
  - `git status`

### PR expectations
A PR description must include:
- What changed (bullets)
- Why (goal/context)
- How verified (exact commands + results)
- Risk areas / follow-ups
- Links/refs: Beads id(s) + Agent Mail thread (if available)

---

## 7) Tool Quick Reference (and how to learn more)

### MCP Agent Mail (coordination + leases)
- Use the MCP tools provided by your harness for:
  - inbox/thread reads, send message, acknowledge
  - file leases/reservations and releases
- Learn more:
  - If the harness supports listing MCP tools, do that.
  - Otherwise ask the user for the local integration commands.

### Beads — `bd` (tasks)
- Start here: `bd quickstart`
- Help: `bd --help`
- Ready work: `bd ready --json`

### Beads Viewer — `bv` (triage / planning)
- IMPORTANT: avoid interactive TUI unless explicitly requested.
- Prefer robot outputs (examples; adjust to your version):
  - `bv --robot-triage`
  - `bv --robot-next`
- Help: `bv --help`

### CASS — `cass` (cross-agent history search)
- IMPORTANT: never run bare `cass` (interactive).
- Prefer:
  - `cass health`
  - `cass search "<q>" --robot --limit 5`
  - `cass capabilities --json`
  - `cass robot-docs guide`
- Help: `cass --help`

### CASS Memory — `cm` (procedural memory)
- Before non-trivial tasks:
  - `cm context "<task>" --json`
- Help: `cm --help`

### Ultimate Bug Scanner — `ubs` (bug scan)
- Preferred: `ubs --staged` (or `ubs --diff`)
- Full scan: `ubs .`
- Help: `ubs --help`

---

## 8) Keeping agent docs healthy (required habit)

If you learn something that will save future time, update `agent_docs/`:
- new command or workflow -> update runbook
- architectural constraint -> add a decision doc
- recurring failure mode -> add to gotchas

Keep `AGENTS.md` short and operational.
Put repo-specific, evolving knowledge in `agent_docs/`.

<!-- bv-agent-instructions-v1 -->

---

## Beads Workflow Integration

This project uses [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) for issue tracking. Issues are stored in `.beads/` and tracked in git.

### Essential Commands

```bash
# View issues (launches TUI - avoid in automated sessions)
bv

# CLI commands for agents (use these instead)
bd ready              # Show issues ready to work (no blockers)
bd list --status=open # All open issues
bd show <id>          # Full issue details with dependencies
bd create --title="..." --type=task --priority=2
bd update <id> --status=in_progress
bd close <id> --reason="Completed"
bd close <id1> <id2>  # Close multiple issues at once
bd sync               # Commit and push changes
```

### Workflow Pattern

1. **Start**: Run `bd ready` to find actionable work
2. **Claim**: Use `bd update <id> --status=in_progress`
3. **Work**: Implement the task
4. **Complete**: Use `bd close <id>` immediately when done (no completed tasks should remain `in_progress`)
5. **Sync**: Always run `bd sync` at session end

### Key Concepts

- **Dependencies**: Issues can block other issues. `bd ready` shows only unblocked work.
- **Priority**: P0=critical, P1=high, P2=medium, P3=low, P4=backlog (use numbers, not words)
- **Types**: task, bug, feature, epic, question, docs
- **Blocking**: `bd dep add <issue> <depends-on>` to add dependencies

### Session Protocol

**Before ending any session, run this checklist:**

```bash
git status              # Check what changed
git add <files>         # Stage code changes
bd close <id> --reason="Completed"  # Close completed beads before syncing
bd sync                 # Commit beads changes
git commit -m "..."     # Commit code
bd sync                 # Commit any new beads changes
git push                # Push to remote
```

### Best Practices

- Check `bd ready` at session start to find available work
- Update status as you work (in_progress → closed immediately on completion)
- Create new issues with `bd create` when you discover tasks
- Use descriptive titles and set appropriate priority/type
- Always `bd sync` before ending session

<!-- end-bv-agent-instructions -->
