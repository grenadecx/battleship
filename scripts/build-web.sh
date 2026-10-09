#!/usr/bin/env bash
# Builds the browser version of the game into a folder any web server can serve:
# index.html, the JavaScript glue and battleship.wasm. The Docker image and CI
# both use this, so the web build is made the same way everywhere.
#
# Usage: scripts/build-web.sh [OUT_DIR]     (default: target/web)
set -euo pipefail

cd "$(dirname "$0")/.."
out=${1:-target/web}
target_dir=${CARGO_TARGET_DIR:-target}

cargo build --release --locked -p battleship --target wasm32-unknown-unknown

# macroquad's JS loader must match the Rust side exactly, so take it from the
# version Cargo.lock pins rather than keeping a copy in the repository.
macroquad=$(cargo pkgid --locked macroquad)
version=${macroquad##*@}
bundle=$(find "${CARGO_HOME:-$HOME/.cargo}/registry/src" -path "*/macroquad-$version/js/mq_js_bundle.js" -print -quit)
[[ -n $bundle ]] || { echo "error: mq_js_bundle.js for macroquad $version not found" >&2; exit 1; }

mkdir -p "$out"
cp web/index.html web/battleship_ws.js "$bundle" "$out/"
cp "$target_dir/wasm32-unknown-unknown/release/battleship.wasm" "$out/"
echo "Web build of macroquad $version in $out:"
ls -l "$out"
