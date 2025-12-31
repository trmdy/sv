# 0002 — Error handling and exit codes

Status: Draft
Date: 2025-12-31
Applies to: src/**

## Context
sv needs consistent error handling with exit codes per spec. We must support
human-friendly errors and structured JSON errors when `--json` is set.

## Decision
- Implement a crate-level error enum (or struct) that categorizes errors as:
  - User error (exit code 2)
  - Policy block (exit code 3)
  - Operation failed (exit code 4)
- Provide `sv::error::Result<T>` and an `ExitCode` mapping helper.
- Attach optional context fields (path, lease id, actor, command) to support
  both human and JSON output.
- In JSON mode, emit a single object with:
  - kind: "user" | "policy" | "operation"
  - message: string
  - context: object (optional)

## Consequences
- Positive: consistent exit codes and structured errors across commands.
- Negative / tradeoffs: slightly more boilerplate when constructing errors.
- Follow-ups: wire JSON output flag to the error renderer in CLI runner.

## Verification
How to confirm this is still true:
- Commands:
  - `sv --json <command-that-errors>`
- Expected result:
  - JSON object with `kind` and `message`, exit code matches spec.

## References
- Related code: src/error.rs, src/cli.rs
- Related tasks: sv-8jf.1.8
- Related coordination thread: <agent mail thread id>
