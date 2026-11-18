#!/usr/bin/env bash
# Copies open-cloud-agents' design/tokens.json (the apps' reference look) into gpui-core/design/,
# and records which commit it came from. Then run `cargo test -p gpui-core --test design_tokens`:
# it fails on every Theme value that no longer matches, and says what to change.
#
# Usage: gpui-core/scripts/sync-tokens.sh <open-cloud-agents checkout>
set -euo pipefail
src=${1:?usage: sync-tokens.sh <open-cloud-agents checkout>}
here=$(cd "$(dirname "$0")/.." && pwd)
cp "$src/design/tokens.json" "$here/design/tokens.json"
rev=$(git -C "$src" log -1 --format=%H -- design/tokens.json)
echo "open-cloud-agents $rev design/tokens.json" > "$here/design/tokens.source"
echo "tokens.json from open-cloud-agents $rev; now: cargo test -p gpui-core --test design_tokens"
