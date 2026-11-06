#!/usr/bin/env bash
# Renders the app icon into web/icons with examples/icon.rs: z^3 on the Power
# example's grid, zoomed out. In the app that is #ex=power&res=4&color=white.
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --example icon
icon=target/debug/examples/icon
out=web/icons

link='#ex=power&res=4'
"$icon" "$out/icon-512.png" 512 3 0.82 "$link"
"$icon" "$out/icon-192.png" 192 3 0.82 "$link"
"$icon" "$out/apple-touch-icon.png" 180 3 0.82 "$link"
# Android crops this one to a shape inside the middle 80%, as wide as a circle.
"$icon" "$out/icon-maskable-512.png" 512 3 0.56 "$link"
# The fine grid turns to mush at this size.
"$icon" "$out/favicon-32.png" 32 18 0.88 '#ex=power&res=2'
