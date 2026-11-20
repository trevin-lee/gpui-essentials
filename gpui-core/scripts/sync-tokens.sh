#!/usr/bin/env bash
# Copies a W3C Design Tokens file into gpui-core/design/tokens.json. Then run
# `cargo test -p gpui-core --test design_tokens`: it fails on every Theme value that no longer
# matches, and says what to change.
#
# Usage: gpui-core/scripts/sync-tokens.sh <tokens.json, or a directory holding one>
set -euo pipefail
src=${1:?usage: sync-tokens.sh <tokens.json, or a directory holding one>}
if [ -d "$src" ]; then
  src="$src/tokens.json"
fi
if [ ! -f "$src" ]; then
  echo "sync-tokens.sh: no tokens file at $src" >&2
  exit 1
fi
here=$(cd "$(dirname "$0")/.." && pwd)
cp "$src" "$here/design/tokens.json"
echo "copied $src; now: cargo test -p gpui-core --test design_tokens"
