# sv — Simultaneous Versioning

Product Specification (Draft v0.1)

## 0. Summary

**sv** is a standalone CLI that makes Git practical for **many parallel agents** by adding:

* **Workspaces** as first-class “agent sandboxes” (Git worktrees)
* **Leases** (graded, descriptive reservations) to reduce duplicate work and merge pain
* **Protected paths** (global “no-edit by default” zones like `.beads/**`)
* **Risk prediction** (detect overlap and simulate conflicts before you’re stuck)
* **Bulk operations** (operate on sets of workspaces at once)
* **Stable Change IDs** (JJ-inspired) to enable continuous “hoisting” without duplicating commits
* **Operation log + undo** (JJ-inspired) to make automation safe

sv intentionally **does not depend** on Beads, MCP mail, or any external coordinator. It exposes generic primitives and optional event output so other systems can integrate without sv taking a hard dependency.

---

## 1. Problem statement

Multi-agent coding in a shared repo fails for predictable reasons:

* Agents **step on each other** (same files / same areas)
* Merge conflicts appear **late** (after lots of work)
* “Who is working on what?” is unclear
* Big, interleaved diffs **reduce velocity**
* Automation mistakes are costly and hard to unwind

Git provides isolation (branches and workspaces (Git worktrees)) but no built-in coordination primitives (leases), no conflict prediction, and no safe “bulk integration” UX.

sv fills those gaps while staying Git-native.

---

## 2. Goals and non-goals

### 2.1 Goals

**Throughput**

* Enable 8–100+ concurrent agents to make progress with minimal blocking.
* Reduce duplicate work and late conflict discovery.

**Safety**

* Make dangerous operations reversible via an operation log + undo.
* Prevent accidental commits of “global” files unless explicitly intended.

**Clarity**

* Provide a single view of “who is touching what”, with intent and lock strength.
* Provide “am I heading toward trouble?” early warnings.

**Git compatibility**

* Uses Git as the backend. Output is ordinary Git commits and branches.
* Must coexist with normal Git workflows (CI, PRs, code review, etc.).
* sv can be adopted incrementally.

**Flexible agent models**

* Supports best practice: **one agent per workspace**.
* Also supports legacy: multiple agents coordinating leases on the **same branch/workspace** (advisory/guard).

### 2.2 Non-goals (v0.x)

* Not a new VCS; no new object database.
* No mandatory integration with any specific coordinator (MCP mail, Slack, etc.).
* No hard real-time global synchronization across machines in v0 (single-machine/local clone first).
* No full JJ revset language. We provide a small selector grammar optimized for sv objects.
* No robust line/chunk locking in v0 (optional “hints” exist, but enforcement is path-level).

---

## 3. Core concepts

### 3.1 Repo

A normal Git repository. sv stores:

* **Tracked config**: `.sv.toml`
* **Local per-workspace state**: `.sv/` (ignored)
* **Shared local state (per clone)**: `.git/sv/` (ignored, shared across workspaces (Git worktrees) in that clone)

### 3.2 Actor

An **actor** is the identity making leases/commits (agent name, username, process ID, etc.).

* Set via `SV_ACTOR`, `--actor`, or `sv actor set`.
* Leases can be **ownerless** (no actor) for “FYI hot zone” flags.

### 3.3 Workspace

A workspace is typically a **workspace directory (Git worktree)** plus sv metadata:

* `sv ws new` creates a workspace (Git worktree) + a branch (HEAD stays on a branch; no detached HEAD requirement)
* sv can also “register” the current directory as a workspace (`sv ws here`) for single-checkout usage

Workspaces are first-class objects in sv: listable, inspectable, selectable.

### 3.4 Lease

A lease is a **graded reservation** over a pathspec, with intent and description.

Lease fields (v0):

* `id` (uuid)
* `pathspec` (file/dir/glob)
* `strength` (`observe | cooperative | strong | exclusive`)
* `intent` (`bugfix | feature | docs | refactor | rename | format | mechanical | investigation | other`)
* `actor` (optional)
* `scope` (`repo | branch:<name> | ws:<workspace>`)
* `note` (required for `strong|exclusive`, optional otherwise)
* `ttl` and `expires_at`
* `hints` (optional; best-effort): `symbols`, `lines`, freeform

Leases are primarily:

* a coordination signal
* an arbitration mechanism (when desired)
* an input to `sv risk`
* an input to commit-time “don’t stomp active exclusive work” checks

### 3.5 Protected paths

Protected paths are patterns that are “global / special” and should not be casually committed.

Example defaults commonly include:

* `.beads/**`
* lockfiles (`pnpm-lock.yaml`, `Cargo.lock`, etc.)
* generated manifests

sv can enforce protection by:

* `guard` (default): allow edits but block commits unless explicitly allowed
* `readonly`: make paths read-only in that workspace (Git worktree)
* `warn`: only warn

### 3.6 Change ID (JJ-inspired)

sv uses a stable Change ID to track “the same logical change” across rebases/cherry-picks.

* Default representation: commit trailer `Change-Id: <uuid>`
* sv ensures a Change-Id exists on `sv commit` (and preserves it during sv-assisted rewrite operations)

Change IDs enable:

* continuous hoist without duplicate commits
* stable references for agents (“I’m on Change-Id X”)

### 3.7 Operations + undo (JJ-inspired)

sv records high-level operations (create workspace, rebase, hoist, etc.) in an **operation log** and supports `sv undo`.

---

## 4. User workflows

### 4.1 Recommended swarm workflow (8 agents, 8 workspaces)

1. Integrator (or orchestrator) creates workspaces:

```bash
sv ws new agent1
sv ws new agent2
...
sv ws new agent8
```

2. Each agent sets identity:

```bash
sv actor set agent3
```

3. Agent takes leases describing work:

```bash
sv take src/auth/** --strength cooperative --intent bugfix --note "Fix refresh edge case"
```

4. Agent codes/tests normally.

5. Agent commits via sv wrapper:

```bash
sv commit -m "Fix token refresh expiry edge case"
```

sv:

* checks protected-path rules
* checks “active incompatible leases owned by others”
* injects Change-Id if missing
* delegates to `git commit`

6. Agent runs a preflight conflict check:

```bash
sv risk
sv risk --simulate
```

7. If needed, rebase onto another workspace to resolve conflicts earlier:

```bash
sv onto agent5
```

8. Integrator continuously “hoists” workspaces onto main into an integration branch:

```bash
sv hoist -s 'ws(active) & ahead("main")' -d main --strategy stack
```

### 4.2 Legacy workflow: multiple agents on the same workspace/branch

sv supports leases in a shared checkout, but does **not** make Git safe for truly concurrent commits in one directory. This mode is primarily for:

* coordinating who edits what
* warning and blocking stomps
* giving agents visibility via `sv risk`

Typical pattern:

* multiple agents run with different `SV_ACTOR`
* they take/release leases to coordinate
* one “committer” (human or agent) performs the actual commits

---

## 5. CLI design

### 5.1 Global CLI conventions

* sv is a standalone tool: `sv …`
* Global flags:

  * `--repo <path>` (optional)
  * `--actor <id>`
  * `--json` (machine-readable output)
  * `--quiet`, `--verbose`
* Exit codes (v0 proposal):

  * `0` success
  * `2` user error (bad args, missing repo)
  * `3` blocked by policy (protected paths, active exclusive lease conflict)
  * `4` operation failed (git error, merge conflict during hoist/onto without resolution policy)

### 5.2 Workspace commands

#### `sv ws new <name> [--base <ref>] [--dir <path>] [--branch <ref>] [--sparse <pathspec...>]`

Creates:

* a workspace directory (Git worktree)
* a Git branch (default: `sv/ws/<name>`)
* registers it in sv workspace registry

Default:

* full checkout (no sparse)
* base ref from `.sv.toml` (`main` by default)

#### `sv ws here [--name <name>]`

Registers the current directory as a workspace (for single-checkout usage).

#### `sv ws list [-s <selector>]`

Lists workspaces with:

* name, path, branch, base, actor (if set), status (ahead/behind), last activity

#### `sv ws info <name>`

Detailed info: touched paths, leases affecting it, ahead/behind, recent Change-Ids, etc.

#### `sv ws rm <name> [--force]`

Removes workspace (Git worktree) and unregisters it (does not delete commits).

---

### 5.3 Task commands

Tasks are repo-scoped work items tracked in `.tasks/`. Status values are
configurable in `.sv.toml` (defaults: `open`, `in_progress`, `closed`).
Tasks can also store lightweight relationships (parent, blocks, described relations).

#### `sv task new <title> [--status <s>] [--body <text>]`

Create a task.

#### `sv task list [--status <s>] [--workspace <name|id>] [--actor <name>] [--updated-since <rfc3339>]`

List tasks with optional filters.

#### `sv task show <id>`

Show task details (including comments).

#### `sv task start <id>`

Mark task as in progress and associate it with the current workspace/branch.

#### `sv task status <id> <status>`

Set a task status (must be one of configured statuses).

#### `sv task close <id> [--status <s>]`

Close a task (status defaults to first entry in `closed_statuses`).

#### `sv task comment <id> <text>`

Add a comment to a task.

#### `sv task parent set <child> <parent>`

Set a parent task.

#### `sv task parent clear <child>`

Clear a parent task.

#### `sv task block <blocker> <blocked>`

Record a blocking relationship.

#### `sv task unblock <blocker> <blocked>`

Remove a blocking relationship.

#### `sv task relate <left> <right> --desc <text>`

Relate two tasks with a description (non-blocking).

#### `sv task unrelate <left> <right>`

Remove a non-blocking relation.

#### `sv task relations <id>`

Show task relationships (parent, children, blocks, relates).

#### `sv task sync`

Merge tracked/shared logs and rebuild snapshot after pulling from other machines.

#### `sv task compact [--older-than <dur>] [--max-log-mb <mb>] [--dry-run]`

Compact task history by collapsing intermediate status changes for closed tasks.

#### `sv task prefix [<prefix>]`

Show or set the repo task ID prefix (alphanumeric).

---

## 6. Lease system

### 6.1 Commands

#### `sv take <pathspec...> [--strength <lvl>] [--intent <kind>] [--scope <scope>] [--ttl <dur>] [--note <text>] [--hint-lines ...] [--hint-symbol ...]`

Creates leases.

Defaults:

* `strength=cooperative`
* `intent=other`
* `scope=repo`
* `ttl=2h` (configurable)

#### `sv release <lease-id...> | sv release <pathspec...>`

Releases a lease.

#### `sv lease ls [-s <selector>]`

Shows active leases.

#### `sv lease who <path>`

Shows who holds overlapping leases on that path, with strength/intent.

#### `sv lease renew <lease-id...> [--ttl <dur>]`

Extends leases.

#### `sv lease break <lease-id...> --reason <text>`

“Break-glass” override (audited in op log).

### 6.2 Strength compatibility rules (default)

* `observe` overlaps with anything
* `cooperative` overlaps with `observe` and `cooperative`
* `strong` overlaps with `observe`; overlaps with `cooperative` only with `--allow-overlap`; blocks `strong/exclusive`
* `exclusive` blocks any overlapping lease except `observe` (and only if configured)

sv must allow policy override in `.sv.toml`.

### 6.3 Ownership

Leases may have `actor` or be ownerless. Ownerless leases behave like “shared warnings.”

### 6.4 Commit-time behavior (critical requirement)

sv **must not** require “currently leased files only” to commit, because workflows may release leases before a later commit step.

Instead, `sv commit` checks:

* if the commit includes paths currently under **active incompatible strong/exclusive leases owned by other actors**, block (or warn depending on policy)
* optionally warn if committed paths were never leased recently (provenance warning; default warn-only)

---

## 7. Protected paths

### 7.1 Commands

#### `sv protect status`

Show current protection patterns and active mode.

#### `sv protect add <pattern...> [--mode guard|readonly|warn]`

Updates `.sv.toml`.

#### `sv protect off <pattern...> [--workspace]`

Disable protection for current workspace only (stored in `.sv/`).

### 7.2 Default behavior

Recommended defaults:

* `.beads/**` protected `guard`
* lockfiles `guard` (configurable)
* generated output directories `warn` or `guard`

---

## 8. Commit wrapper

### 8.1 `sv commit [sv-flags...] -- [git commit args...]`

sv should support pass-through to git commit. In v0, at minimum support:

* `-m`, `-F`, `--amend`, `-a`, `--no-edit`

Behavior:

1. Determine the set of paths that would be committed (typically via `git diff --cached --name-only`, after any `-a` behavior is applied).
2. Enforce protected paths:

   * block if protected paths present unless explicit override
3. Enforce active incompatible lease conflicts:

   * block if a file in the commit is currently under active `exclusive` (or `strong`, depending on config) lease owned by another actor
4. Ensure Change-Id trailer exists (inject if missing).
5. Execute `git commit` with the finalized message.

**Note:** Handling interactive editor mode is desirable but can be phased:

* v0: support `-m/-F` cleanly (agents typically use these)
* v0.2: support editor by using an editor wrapper that appends Change-Id on save

---

## 9. Risk prediction

### 9.1 `sv risk [-s <selector>] [--base <ref>] [--simulate]`

Purpose:

* Detect overlap and predict conflict pain before it happens.

#### Without `--simulate` (fast mode)

For each selected workspace:

* compute touched files relative to base: `git diff --name-only <base>..<ws-branch>`
* intersect touched sets across workspaces
* incorporate lease metadata (strength/intent) for risk scoring

Output:

* overlap summary by file/dir
* “hot” overlaps (strong/exclusive, format/rename)
* suggested actions:

  * take lease / downgrade lease / rebase onto other workspace / pick another task

#### With `--simulate` (conflict simulation)

For selected pairs or for “stack onto base”:

* run a virtual merge simulation (no working tree mutation) to detect true conflicts
* report files with conflicts and conflict type (content/add-add/etc.)
* optionally emit machine-readable JSON for automation

---

## 10. Onto (workspace-to-workspace rebase/merge)

### 10.1 `sv onto <target-workspace> [--strategy rebase|merge|cherry-pick] [--base <ref>]`

Repositions current workspace branch on top of the target’s tip.

Requirements:

* records operation in op log
* supports `sv undo`
* optional preflight `sv risk --simulate` integration

Default strategy:

* `rebase` (keeps linear history per workspace)

---

## 11. Hoist (bulk integration)

### 11.1 `sv hoist -s <selector> -d <dest-ref> --strategy <stack|rebase|merge> [--order <mode>] [--continue-on-conflict]`

Purpose:

* take many workspace branches and produce an updated integrated view on top of `main` (or other base) repeatedly.

#### `stack` strategy (recommended default)

* creates/updates an integration branch: `sv/hoist/<dest-ref>`
* resets integration branch to `<dest-ref>`
* selects commits from chosen sources
* deduplicates by Change-Id:

  * if duplicates have identical patch-id → include once
  * if duplicates diverge → choose by policy or require user to resolve with `--prefer <workspace>`; emit warnings
* replays (cherry-picks) commits in deterministic order onto integration branch

Properties:

* idempotent (rebuild branch each run)
* does not rewrite source workspaces
* enables continuous “hoist new work as it appears”

#### `rebase` strategy

* rebases each selected workspace onto dest directly (more invasive; can disrupt workers)

#### `merge` strategy

* merges workspace tips into dest (not linear; optional)

### 11.2 Ordering modes

* `workspace` (default): stable sort by workspace name; preserve commit order within each workspace
* `time`: sort by commit time (less deterministic across machines)
* `explicit`: take an ordered list of workspaces or a config-defined priority list

---

## 12. Selectors (revset-inspired, sv-scoped)

sv introduces a small selector language for operating over sets of workspaces/leases/branches. This is inspired by JJ’s “operate on sets,” without importing full revsets.

### 12.1 Examples

* `ws(active)`
* `ws(active) & ahead("main")`
* `ws(name~"agent") & touching("src/auth/**")`
* `lease(active) & overlaps("src/auth/**")`
* `ws(active) ~ ws(blocked)`

### 12.2 Semantics (v0)

* Entities: `ws(...)`, `lease(...)`, `branch(...)`
* Predicates:

  * `active`, `stale`
  * `name~"regex"`
  * `ahead("ref")`
  * `touching("pathspec")`
  * `blocked` (policy violations, conflicts, etc.)
* Operators: `|` union, `&` intersection, `~` difference, parentheses

Selectors must be supported by:

* `sv ws list -s`
* `sv risk -s`
* `sv hoist -s`
* `sv lease ls -s`

---

## 13. State and storage layout

### 13.1 Tracked files

* `.sv.toml` (tracked)

  * base branch default
  * protected path patterns + enforcement
  * lease policy defaults
  * selector defaults (optional)
  * hoist defaults (optional)

* `.tasks/` (tracked)

  * task log + snapshot (`tasks.jsonl`, `tasks.snapshot.json`)

### 13.2 Workspace-local (ignored)

* `.sv/` (ignored)

  * workspace name/id
  * per-workspace overrides (protected paths off, mode)
  * optional local logs

### 13.3 Shared-local (ignored, per clone)

* `.git/sv/`

  * `workspaces.json` (registry; can be derived but cached)
  * `leases.sqlite` or `leases.jsonl` (active leases + history)
  * `oplog/` (append-only operation records)
  * `hoist/` (hoist state, conflict records)

### 13.4 Concurrency and locking

sv must assume multiple processes may run concurrently (multiple agents).

Implementation requirement:

* Use file locks around `.git/sv` writes
* Use atomic writes (write temp + rename)
* Prefer SQLite for lease registry if concurrency becomes messy; JSONL with locks is acceptable initially

---

## 14. Operation log + undo

### 14.1 `sv op log`

Shows operations with:

* op id, timestamp, actor, command, affected refs/workspaces, outcome

### 14.2 `sv undo [--op <id>]`

Undo semantics:

* If last op moved refs: move them back
* If last op created a workspace: remove it (optionally keep with `--keep-worktree`, which keeps the workspace directory)
* If last op was hoist: restore integration branch to previous tip
* If last op was onto/rebase: restore branch refs to previous commit(s)

sv must record enough information to undo safely:

* old/new ref tips
* created/deleted workspace paths (Git worktree paths)
* lease changes

---

## 15. Interoperability guarantees

sv must not “lock users in.”

* The repo remains a standard Git repo.
* Workspaces map to Git's native workspace mechanism (Git worktrees); can be managed by Git directly if needed.
* Leases and policies are additive; they don’t change Git history format.
* If sv disappears, the repo remains usable with Git alone.

---

## 16. Roadmap

### v0.1 (MVP)

* Workspaces: `ws new`, `ws list`, `ws info`, `ws rm`, `ws here`
* Leases: `take`, `release`, `lease ls`, `lease who`, TTL, actor
* Protected paths: `.sv.toml`, guard mode, per-workspace override
* `sv commit` wrapper with:

  * protected-path blocking
  * active incompatible lease stomping check
  * Change-Id injection (`-m/-F` focus)
* `sv risk` fast mode (diff overlap + lease-aware risk scoring)
* Op log + basic undo for workspace create/remove and ref moves

### v0.2

* `sv risk --simulate` (virtual merge conflict prediction)
* `sv onto` with undo support
* Hoist `stack` strategy (dedup by Change-Id + patch-id)
* Selector language v1 across ws/lease

### v0.3+

* Better editor-mode support for Change-Id injection
* Optional hard mode (chmod protected paths read-only)
* Daemon option for lease renewals + event stream (still optional)

### v1.0

* Remote/shared lease backend plugin interface (optional)
* Advanced conflict workflows (conflict artifact branches, resolver roles)
* Optional “semantic hints” (symbols) integration for risk scoring

---

## 17. Open questions to settle early

1. **Default enforcement mode**

   * Advisory vs Guard vs Readonly by default for protected paths and scope drift.
   * Recommendation: Protected = `guard`, Leases = `warn+block only for exclusive/strong`.

2. **Lease store format**

   * SQLite (strong concurrency) vs JSONL (simpler).
   * Recommendation: start JSONL + file lock, migrate to SQLite if needed.

3. **Change-Id policy**

   * Mandatory for all commits created via sv (recommended) vs optional.
   * Recommendation: mandatory in `sv commit` (it’s foundational for hoist).

4. **Hoist ordering**

   * Default order by workspace name is deterministic but may not match dependency order.
   * Recommendation: allow `--order workspace|explicit`, keep deterministic default.

5. **Behavior in “same workspace multi-agent”**

   * sv can coordinate leases, but Git cannot safely support simultaneous commits.
   * Recommendation: document clearly; encourage separate workspaces (Git worktrees) for real parallelism.

---

## Appendix A: Suggested `.sv.toml` (starter)

```toml
base = "main"

[actor]
default = "unknown"

[leases]
default_strength = "cooperative"
default_intent = "other"
default_ttl = "2h"

[leases.compat]
# policy knobs; exact schema TBD
allow_overlap_cooperative = true
require_flag_for_strong_overlap = true

[protect]
mode = "guard"
paths = [
  ".beads/**",
  "pnpm-lock.yaml",
  "package-lock.json",
  "Cargo.lock",
]
```
