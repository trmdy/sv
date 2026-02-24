# Testing Guide

This document summarizes the current test setup for sv.
For authoritative commands, see `agent_docs/runbooks/test.md`.

## Running tests

```bash
make test        # Library tests only (fast)
make test-all    # All tests including integration
cargo test       # Same as make test-all
```

## Test layout
- `tests/support/`: shared helpers for integration tests
- `tests/fixtures.rs`: sanity check for temp repo creation
- `tests/unit_config.rs`: config parsing and defaults
- `tests/unit_error.rs`: error types and exit code mapping

## Concurrency tests
Locking tests live in `src/lock.rs` under `#[cfg(test)]` and exercise:
- concurrent lock acquisition behavior
- timeout handling
- atomic write consistency under contention

## Adding new tests
- Prefer `tests/` for integration-style tests
- Unit tests can live alongside modules when tight coupling is useful
- Keep tests deterministic and avoid touching networked resources

---

## Manual Testing

### 1. Set up a test repository

```bash
cd /tmp
mkdir test-sv && cd test-sv
git init
git commit --allow-empty -m "Initial commit"
sv init
```

### 2. Test actor commands

```bash
# Set actor identity
sv actor set agent1
sv actor show

# Test environment variable override
SV_ACTOR=envactor sv actor show

# Test CLI flag override
sv --actor cliactor actor show
```

### 3. Test status command

```bash
sv status
sv status --json
```

### 4. Test lease commands

```bash
# Take a lease
sv take src/ --strength cooperative --intent feature
sv take "*.rs" --strength strong --intent refactor --note "Refactoring modules"

# List leases
sv lease ls
sv lease ls --json

# Check who holds a lease
sv lease who src/

# Release leases
sv release src/
```

### 5. Test workspace commands

```bash
# Register current directory as workspace
sv ws here --name main-ws

# List workspaces
sv ws list
sv ws list --json

# Show workspace info
sv ws info main-ws

# Resolve workspace path for switching
sv ws switch main-ws --path
sv ws switch --path

# Create a new workspace (worktree)
sv ws new agent2 --base main

# Remove a workspace
sv ws rm agent2
```

### 6. Test risk assessment

```bash
# Basic risk check
sv risk
sv risk --json

# With specific base ref
sv risk --base main

# Simulate merge conflicts
sv risk --simulate
sv risk --simulate --json
```

### 7. Test with multiple workspaces

```bash
# Set up test repo
cd /tmp
rm -rf multi-ws-test && mkdir multi-ws-test && cd multi-ws-test
git init
echo "base content" > shared.txt
git add shared.txt
git commit -m "Initial commit"
sv init

# Register main workspace
sv ws here --name main-ws

# Create second workspace
sv ws new agent2 --base main
cd ../agent2

# Make overlapping changes
echo "agent2 changes" > shared.txt
git add shared.txt
git commit -m "Agent2 changes"

# Check for conflicts
cd ../multi-ws-test
sv risk
sv risk --simulate
```

### 8. Test onto command

```bash
# Preview conflicts before rebasing
sv onto agent2 --preflight
sv onto agent2 --preflight --json

# Execute rebase (use with caution)
sv onto agent2 --strategy rebase
```

### 9. Test hoist command

```bash
# Dry run
sv hoist -s all -d main --dry-run
sv hoist -s all -d main --dry-run --json

# With continue-on-conflict flag
sv hoist -s all -d main --continue-on-conflict --dry-run
```

### 10. Test protected paths

```bash
# Add protected path
sv protect add "*.lock" --mode warn

# Check protection status
sv protect status
sv protect status --json

# Remove protection
sv protect rm "*.lock"
```

### 11. Test commit wrapper

```bash
# Stage some changes
echo "test" > test.txt
git add test.txt

# Commit with sv (adds Change-Id)
sv commit -m "Test commit"

# Check the commit message
git log -1
```

## Testing Error Handling

```bash
# Test in non-git directory
cd /tmp
sv init --json
# Expected: exit code 2, error about not finding repo

# Test invalid arguments
sv take foo --strength invalid
# Expected: exit code 2, error about invalid strength

# Test missing workspace
sv ws info nonexistent
# Expected: exit code 2, workspace not found
```

## Test Output Formats

All commands support `--json` for machine-readable output:

```bash
sv status --json
sv lease ls --json
sv risk --json
sv ws list --json
```

Use `--quiet` to suppress human-readable output:

```bash
sv take src/ --quiet
sv init --quiet
```

## Cleanup

```bash
# Remove test directories
rm -rf /tmp/test-sv /tmp/multi-ws-test

# Uninstall sv
make uninstall
```
