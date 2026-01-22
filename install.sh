#!/usr/bin/env bash
set -euo pipefail

REPO="${SV_REPO:-tOgg1/sv}"
VERSION="${SV_VERSION:-}"
BINDIR="${SV_BINDIR:-$HOME/.local/bin}"

usage() {
  cat <<'EOF'
sv install (linux)

Usage:
  ./install.sh [--version vX.Y.Z] [--bindir PATH] [--repo owner/name]

Env:
  SV_VERSION  Version to install (default: latest release)
  SV_BINDIR   Install directory (default: ~/.local/bin)
  SV_REPO     GitHub repo (default: tOgg1/sv)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --bindir)
      BINDIR="${2:-}"
      shift 2
      ;;
    --repo)
      REPO="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This installer targets Linux only." >&2
  exit 1
fi

arch="$(uname -m)"
case "$arch" in
  x86_64|amd64)
    target="x86_64-unknown-linux-gnu"
    ;;
  *)
    echo "Unsupported architecture: $arch (supported: x86_64)" >&2
    exit 1
    ;;
esac

download() {
  local url="$1"
  local out="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "$out" "$url"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -qO "$out" "$url"
    return
  fi

  echo "Need curl or wget to download releases." >&2
  exit 1
}

download_stdout() {
  local url="$1"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -qO- "$url"
    return
  fi

  echo "Need curl or wget to download releases." >&2
  exit 1
}

if [[ -z "$VERSION" ]]; then
  api="https://api.github.com/repos/${REPO}/releases/latest"
  VERSION="$(
    download_stdout "$api" \
      | grep -m1 '"tag_name"' \
      | sed -E 's/.*"([^"]+)".*/\1/'
  )"
fi

VERSION="${VERSION#v}"
if [[ -z "$VERSION" ]]; then
  echo "Could not resolve version." >&2
  exit 1
fi

asset="sv-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/v${VERSION}/${asset}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

download "$url" "$tmpdir/$asset"
tar -xzf "$tmpdir/$asset" -C "$tmpdir"

mkdir -p "$BINDIR"
install -m 0755 "$tmpdir/sv" "$BINDIR/sv"

if ! command -v sv >/dev/null 2>&1; then
  echo "Installed sv to $BINDIR/sv"
  echo "Add $BINDIR to PATH if needed."
else
  echo "Installed sv to $BINDIR/sv"
fi
