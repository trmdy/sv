# sv <-> forge integration

## Goals
- sv owns tasks
- forge loop just needs "current task pointer" + status for prompt injection + dashboards
- forge stays task-tech-agnostic (task ids opaque strings)

## Forge: work context primitive (implemented)
Forge stores per-loop work context in its DB:
- agent_id (default: FMAIL_AGENT / SV_ACTOR / FORGE_LOOP_NAME)
- task_id (opaque: sv-..., jira-..., linear-..., markdown filename, etc)
- status (opaque string)
- updated_at
- loop_iteration (autoset from loop metadata iteration_count)
- is_current (single current pointer per loop)

Commands:
- forge work set <task-id> --status <status> --detail "..." [--loop <loop-ref>] [--agent <agent-id>]
- forge work current
- forge work ls
- forge work clear

Defaults:
- --loop defaults to $FORGE_LOOP_ID (when run inside a forge loop run)
- --agent defaults to $FMAIL_AGENT, else $SV_ACTOR, else $FORGE_LOOP_NAME

Prompt injection:
- forge loop appends a "Loop Context (persistent)" section from:
  - current + recent forge work context
  - forge per-loop kv memory (forge mem)

Env injection in forge loop runs (implemented):
- SV_REPO=<repoPath>
- SV_ACTOR=<loop-name> (defaults to FMAIL_AGENT)

## sv: hook runner (implemented)
sv runs best-effort shell hooks on task lifecycle events (never blocks task ops).

Currently wired events:
- `sv task start` -> `integrations.forge.on_task_start.cmd`
- `sv task block` -> `integrations.forge.on_task_block.cmd`
- `sv task close` -> `integrations.forge.on_task_close.cmd`

Minimal config idea (.sv.toml):
```toml
[integrations.forge]
enabled = true

# how to select forge loop for this repo/actor
loop_ref = "{actor}"       # common: forge loop name == sv actor
# or: loop_ref = "review-loop"

[integrations.forge.on_task_start]
cmd = "forge work set {task_id} --status in_progress --loop {loop_ref} --agent {actor}"

[integrations.forge.on_task_block]
cmd = "forge work set {task_id} --status blocked --loop {loop_ref} --agent {actor}"

[integrations.forge.on_task_close]
cmd = "forge work set {task_id} --status done --loop {loop_ref} --agent {actor} && forge work clear --loop {loop_ref}"
```

Placeholders:
- {task_id}  sv task id (sv-...)
- {actor}    sv actor (SV_ACTOR)
- {loop_ref} derived from integrations.forge.loop_ref

Failure policy:
- never block sv task operations if forge not installed / command fails

## sv UX command idea
## sv UX command (implemented)
- `sv forge hooks install`
  - writes/updates `[integrations.forge]` block in `.sv.toml`
  - defaults: `loop_ref="{actor}"`
  - flags:
    - `--loop <loop-ref>`
    - `--status-map open=in_progress,blocked=blocked,closed=done`

## Notes
- forge does not call sv; sv may call forge.
- task status mapping is up to the hook config.
