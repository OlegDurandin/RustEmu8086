#!/bin/sh
# Trunk post_build hook (Trunk.toml): puts the size of the optimized wasm into index.html.
set -e
cd "$TRUNK_STAGING_DIR"
for wasm in *_bg.wasm; do
  size=$(wc -c < "$wasm" | tr -d ' ')
  perl -pi -e "s|('[^']*\Q$wasm\E', )\d+|\${1}$size|" index.html
done
