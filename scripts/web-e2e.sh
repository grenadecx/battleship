#!/usr/bin/env bash
# Plays the opening of an online game in two headless Chromes against a running
# web server (for example the Docker image), using scripts/web-e2e.mjs.
#
# Usage: scripts/web-e2e.sh [URL] [OUT_DIR]   (defaults: http://localhost:8080, target/web-e2e)
# Needs Node 22+ and Chrome; set CHROME to use another Chrome or Chromium binary.
set -euo pipefail

cd "$(dirname "$0")/.."
url=${1:-http://localhost:8080}
out=${2:-target/web-e2e}
chrome=${CHROME:-google-chrome}
mkdir -p "$out"

profiles=$(mktemp -d)
pids=()
cleanup() {
    kill "${pids[@]}" 2>/dev/null || true
    wait "${pids[@]}" 2>/dev/null || true
    rm -rf "$profiles"
}
trap cleanup EXIT

# One browser per player: background tabs get no animation frames, so the game
# in a second tab of the same browser would not run.
for port in 9223 9224; do
    "$chrome" --headless=new --user-data-dir="$profiles/$port" --no-first-run \
        --remote-debugging-port="$port" --enable-unsafe-swiftshader --use-angle=swiftshader \
        --window-size=1200,760 about:blank >/dev/null 2>&1 &
    pids+=($!)
done
for port in 9223 9224; do
    for _ in $(seq 50); do
        curl -fs "http://127.0.0.1:$port/json/version" >/dev/null && break
        sleep 0.1
    done
done

node scripts/web-e2e.mjs 9223 9224 "$url" "$out"
echo "Screenshots in $out"
