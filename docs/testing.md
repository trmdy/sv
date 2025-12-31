# Testing Guide

This document summarizes the current test setup for sv.
For authoritative commands, see `agent_docs/runbooks/test.md`.

## Running tests
- Unit and integration tests: `cargo test`

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
