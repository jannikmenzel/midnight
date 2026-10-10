#!/usr/bin/env bash
set -euo pipefail

dir="$(cd "$(dirname "$0")" && pwd)"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

for scale in 1 2; do
  "$chrome" --headless=new --disable-gpu --hide-scrollbars --allow-file-access-from-files \
    --force-device-scale-factor="$scale" --window-size=660,400 --virtual-time-budget=3000 \
    --screenshot="$tmp/bg@${scale}x.png" "file://$dir/background.html" >/dev/null 2>&1
done

tiffutil -cathidpicheck "$tmp/bg@1x.png" "$tmp/bg@2x.png" -out "$dir/background.tiff" >/dev/null
echo "$dir/background.tiff"
