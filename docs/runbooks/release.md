# Release / Deploy Runbook

Status: evolving
Last verified: YYYY-MM-DD

## One-time setup
- GitHub Actions secret: `HOMEBREW_TAP_GITHUB_TOKEN` (PAT with repo access to tap/cask repo)
- GitHub Actions variable: `HOMEBREW_TAP_REPO` (default: `trmdy/homebrew-tap`)
- Optional variables:
  - `HOMEBREW_FORMULA_PATH` (default: `Formula/sv.rb`)
  - `HOMEBREW_CASK_PATH` (set if you maintain a cask)

## Deploy steps
1) Ensure main is clean + CI green.
2) Bump version in `Cargo.toml` (and `Cargo.lock`).
3) Tag + push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
4) Wait for GitHub `Release` workflow to publish assets + update Homebrew tap/cask.
5) Verify assets exist for:
   - `sv-x86_64-unknown-linux-gnu.tar.gz`
   - `sv-x86_64-apple-darwin.tar.gz`
   - `sv-aarch64-apple-darwin.tar.gz`
   - `sv-x86_64-pc-windows-msvc.zip`
6) Sanity check:
   - Linux: `curl -fsSL https://raw.githubusercontent.com/tOgg1/sv/main/install.sh | bash`
   - macOS: `brew tap trmdy/homebrew-tap && brew install sv` (or your tap)

## Verification
- Check `sv --version` on each platform.
- If release assets are wrong, delete the release tag + re-tag after fix.

## “Do not touch without approval”
- N/A
