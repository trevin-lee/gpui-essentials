#!/usr/bin/env bash
# Copies the host app's design/tokens.json (the apps' reference look) into gpui-core/design/,
# and records which commit it came from. Then run `cargo test -p gpui-core --test design_tokens`:
# it fails on every Theme value that no longer matches, and says what to change.
#
# Usage: gpui-core/scripts/sync-tokens.sh <host-app checkout>
set -euo pipefail
src=${1:?usage: sync-tokens.sh <host-app checkout>}
here=$(cd "$(dirname "$0")/.." && pwd)
cp "$src/design/tokens.json" "$here/design/tokens.json"
rev=$(git -C "$src" log -1 --format=%H -- design/tokens.json)
echo "host-app $rev design/tokens.json" > "$here/design/tokens.source"
echo "tokens.json from host-app $rev; now: cargo test -p gpui-core --test design_tokens"
