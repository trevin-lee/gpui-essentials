#!/bin/sh
# Builds the live page's code into the folder given: the Rust for wasm32, its JavaScript glue
# (wasm-bindgen), and the wasm shrunk with wasm-opt. Needs the wasm32-unknown-unknown target,
# wasm-bindgen-cli at the version Cargo.toml pins, and binaryen's wasm-opt.
#
#   docs-site/live/build.sh $TMPDIR/site/live
set -eu
out=$1
here=$(cd "$(dirname "$0")" && pwd)
target=${CARGO_TARGET_DIR:-$here/target}
cargo build --manifest-path "$here/Cargo.toml" --release --locked --target wasm32-unknown-unknown
mkdir -p "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name live \
    "$target/wasm32-unknown-unknown/release/docs_site_live.wasm"
# The features Rust's wasm32 target uses by default, so wasm-opt keeps (and may use) them.
wasm-opt -Oz --strip-debug --strip-producers \
    --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    --enable-mutable-globals --enable-reference-types --enable-multivalue \
    -o "$out/live_bg.wasm" "$out/live_bg.wasm"
ls -l "$out"
