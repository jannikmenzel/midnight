#!/bin/sh
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$(cd "$(dirname "${1:-.}")" && pwd)/$(basename "${1:-THIRD_PARTY_NOTICES.txt}")"
npm_notices="$(mktemp)"
trap 'rm -f "$npm_notices"' EXIT
cd "$root"

cargo about generate --manifest-path src-tauri/Cargo.toml -c src-tauri/about.toml -o "$out" src-tauri/about.hbs
npx generate-license-file -c scripts/glf.json --input package.json --output "$npm_notices" --overwrite
cat "$npm_notices" >> "$out"
