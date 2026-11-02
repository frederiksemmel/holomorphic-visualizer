#!/usr/bin/env bash
# Builds the browser version into web/dist, ready to serve as static files.
# Needs the wasm32-unknown-unknown Rust target and wasm-bindgen-cli at the
# version of the wasm-bindgen crate in Cargo.lock; uses wasm-opt if present.
set -euo pipefail
cd "$(dirname "$0")/.."

out=web/dist
cargo build --profile web --target wasm32-unknown-unknown --bin conformal-visualizer

rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name app \
  "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/web/conformal-visualizer.wasm"
if command -v wasm-opt >/dev/null; then
  # Features come from the target_features section that rustc emits.
  wasm-opt -Oz --strip-debug --strip-producers -o "$out/app_bg.wasm" "$out/app_bg.wasm"
fi
cp web/index.html web/preview.gif web/social.png "$out/"
# The loading screen measures its progress against the unpacked size.
sed -i "s/data-wasm-size=\"\"/data-wasm-size=\"$(stat -c %s "$out/app_bg.wasm")\"/" "$out/index.html"
du -h "$out"/*
