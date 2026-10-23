#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "Usage: scripts/bump-version.sh <version>" >&2
  echo "Example: scripts/bump-version.sh 0.1.2" >&2
  exit 1
fi

VERSION="${VERSION#v}"

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
  echo "Invalid version: $VERSION (expected semver like 0.1.2)" >&2
  exit 1
fi

perl -pi -e 's/^version = ".*"/version = "'"$VERSION"'"/' src-tauri/Cargo.toml

tmp="$(mktemp)"
jq --arg v "$VERSION" '.version = $v' src-tauri/tauri.conf.json > "$tmp"
mv "$tmp" src-tauri/tauri.conf.json

npm version "$VERSION" --no-git-tag-version --allow-same-version

echo "Bumped to $VERSION:"
echo "  src-tauri/Cargo.toml"
echo "  src-tauri/tauri.conf.json"
echo "  package.json (+ package-lock.json)"
LOCK="src-tauri/Cargo.lock"
if [[ -f "$LOCK" ]]; then
  # Only the root package version changes; edit the lock directly so no network
  # or re-resolution is needed. Run `cargo check` if dependencies changed too.
  perl -0pi -e 's/(\[\[package\]\]\nname = "mole-desktop"\nversion = ")[^"]*(")/${1}'"$VERSION"'${2}/' "$LOCK"
  echo "  src-tauri/Cargo.lock (updated)"
fi
