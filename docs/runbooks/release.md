# Release / Deploy Runbook

Status: evolving
Last verified: YYYY-MM-DD

## Deploy steps
1) Ensure main is clean + CI green.
2) Bump version in `Cargo.toml` (and `Cargo.lock`).
3) Tag + push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
4) Wait for GitHub `Release` workflow to publish assets.
5) Verify assets exist for:
   - `sv-x86_64-unknown-linux-gnu.tar.gz`
   - `sv-x86_64-apple-darwin.tar.gz`
   - `sv-aarch64-apple-darwin.tar.gz`
   - `sv-x86_64-pc-windows-msvc.zip`
6) Update `Formula/sv.rb`:
   - set `version`
   - update macOS `sha256` values (from release assets)
   - example:
     - `gh release download vX.Y.Z --pattern 'sv-*-apple-darwin.tar.gz'`
     - `shasum -a 256 sv-*-apple-darwin.tar.gz`
   - commit + push
7) Sanity check:
   - Linux: `curl -fsSL https://raw.githubusercontent.com/tOgg1/sv/main/install.sh | bash`
   - macOS: `brew tap tOgg1/sv https://github.com/tOgg1/sv && brew install sv`

## Verification
- Check `sv --version` on each platform.
- If release assets are wrong, delete the release tag + re-tag after fix.

## “Do not touch without approval”
- N/A
