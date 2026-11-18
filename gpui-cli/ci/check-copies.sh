#!/usr/bin/env bash
# Copies every gpui-core component into a new crate with this checkout's `gpui add`, as a
# host would, and builds it; then runs side_by_side.rs, which puts a copy next to gpui-core's
# original in one app. CI runs it on Linux; it runs on a Mac too.
#
# Usage: gpui-cli/ci/check-copies.sh [scratch-dir]   (from anywhere in the checkout)
set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
scratch=${1:-$(mktemp -d)}
host="$scratch/host"
rm -rf "$host"
mkdir -p "$host/src" "$host/tests"

# GPUI as gpui-core takes it, and gpui-core's other dependencies the copies use, at its versions.
gpui=$(grep -E '^gpui = ' "$repo/Cargo.toml" | sed 's/^gpui = //')
dep() { grep -E "^$1 = " "$repo/gpui-core/Cargo.toml"; }
cat > "$host/Cargo.toml" <<TOML
[package]
name = "copies"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
gpui = $gpui
gpui-core = { path = "$repo/gpui-core" }
$(dep chrono)
$(dep unicode-segmentation)

[dev-dependencies]
gpui = $(echo "$gpui" | sed 's/ }$/, features = ["test-support"] }/')

# Not part of the checkout's workspace.
[workspace]
TOML
printf 'pub mod ui;\n' > "$host/src/lib.rs"
# The checkout's lock and Rust, so the copies build with what gpui-core is tested with.
cp "$repo/Cargo.lock" "$host/Cargo.lock"
cp "$repo/rust-toolchain.toml" "$host/"

cli=(cargo run --quiet --locked --manifest-path "$repo/Cargo.toml" -p gpui-cli --)
cd "$host"
"${cli[@]}" add --all --path "$repo"
# A second run changes nothing.
before=$(cat src/ui/*.rs | cksum)
"${cli[@]}" add --all --path "$repo" | tee "$scratch/again.txt"
[ "$(cat src/ui/*.rs | cksum)" = "$before" ] || { echo "a second add changed the copies"; exit 1; }
grep -q '^Copied' "$scratch/again.txt" && { echo "a second add copied again"; exit 1; }
"${cli[@]}" add --all --path "$repo" --diff | grep -q '^No differences' \
  || { echo "--diff found differences right after add"; exit 1; }

# The copies build without a warning, as gpui-core does. (Cargo replays a cached crate's
# warnings; RUSTFLAGS would rebuild GPUI.)
cargo build 2>&1 | tee "$scratch/build.txt"
if grep -E '^(warning|error)' "$scratch/build.txt"; then echo "the copies have warnings"; exit 1; fi
cp "$repo/gpui-cli/ci/side_by_side.rs" tests/
cargo test --test side_by_side
echo "check-copies: every component copied, built, and a copy ran beside gpui-core's own"
