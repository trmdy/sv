# Dev Environment Runbook

Status: evolving
Last verified: 2026-01-11

## Setup
- Install Rust (rustup) and Git
- macOS arm64:
  - `rustup default stable-aarch64-apple-darwin`
  - `brew install openssl@3 pkg-config`
  - `export OPENSSL_DIR="$(brew --prefix openssl@3)"`
  - `export PKG_CONFIG_PATH="$OPENSSL_DIR/lib/pkgconfig"`
- macOS x86_64 (only if targeting x86_64):
  - Install x86_64 Homebrew under `/usr/local`
  - `export OPENSSL_DIR="/usr/local/opt/openssl@3"`
  - `export PKG_CONFIG_PATH="$OPENSSL_DIR/lib/pkgconfig"`
- Linux:
  - Debian/Ubuntu: `sudo apt-get install -y libssl-dev pkg-config`
  - Fedora/RHEL: `sudo dnf install -y openssl-devel pkgconfig`
- Build: `cargo build --release`

## Common issues
- OpenSSL link errors on Apple Silicon with x86_64 toolchain -> use arm64 toolchain or `/usr/local` x86_64 OpenSSL
