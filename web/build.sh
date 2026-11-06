#!/usr/bin/env bash
# Builds the browser version into web/dist, ready to serve as static files.
# Needs the wasm32-unknown-unknown Rust target and wasm-bindgen-cli at the
# version of the wasm-bindgen crate in Cargo.lock; uses wasm-opt if present.
# WEB_PROFILE=web-dev builds much faster, for a bigger download (see Cargo.toml).
set -euo pipefail
cd "$(dirname "$0")/.."

out=web/dist
profile="${WEB_PROFILE:-web}"
cargo build --profile "$profile" --target wasm32-unknown-unknown --bin conformal-visualizer

rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name app \
  "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/$profile/conformal-visualizer.wasm"
if [ "$profile" = web ] && command -v wasm-opt >/dev/null; then
  # Features come from the target_features section that rustc emits.
  wasm-opt -Oz --strip-debug --strip-producers -o "$out/app_bg.wasm" "$out/app_bg.wasm"
fi
cp web/index.html web/manifest.webmanifest web/preview.gif web/social.png web/icons/*.png "$out/"
# The loading screen measures its progress against the unpacked size.
sed -i "s/data-wasm-size=\"\"/data-wasm-size=\"$(stat -c %s "$out/app_bg.wasm")\"/" "$out/index.html"
du -h "$out"/*
