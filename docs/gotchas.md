# Gotchas / Pitfalls

Status: evolving
Last verified: 2026-01-20

Add short entries that prevent repeated failures.

## Format
- **Symptom:** what you see
- **Cause:** why it happens
- **Fix:** exact commands / code pointers

## Entries
- **Symptom:** build fails at link time with OpenSSL errors like `found architecture 'arm64', required architecture 'x86_64'` or `_OPENSSL_init_ssl` missing.
  **Cause:** x86_64 Rust toolchain running on Apple Silicon while Homebrew OpenSSL is arm64 (`/opt/homebrew`).
  **Fix:** use a native arm64 Rust toolchain (e.g., `rustup default stable-aarch64-apple-darwin`) or install x86_64 Homebrew OpenSSL under `/usr/local` and point `OPENSSL_DIR`/`PKG_CONFIG_PATH` there when using the x86_64 toolchain. A macOS preflight now fails early when it detects `/opt/homebrew` for x86_64 builds.
- **Symptom:** after `sv hoist` on the current branch, `git status` shows widespread deletions or missing files even though HEAD advanced.
  **Cause:** worktree checkout skipped because local changes blocked a safe checkout or checkout failed.
  **Fix:** clean local changes and run `git restore --source=HEAD --staged --worktree .` (or rerun hoist with a clean worktree).
