#!/bin/sh
# Turns the static export (out/) into Vercel's Build Output (.vercel/output/), which
# `vercel deploy --prebuilt` uploads as it is: no build on Vercel's side.
#
#   npm run build && scripts/vercel-output.sh && vercel deploy --prebuilt
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
output=$here/.vercel/output
rm -rf "$output"
mkdir -p "$output"
cp -R "$here/out" "$output/static"
# Files first; anything else is the export's 404 page.
cat > "$output/config.json" <<'JSON'
{
  "version": 3,
  "routes": [
    { "handle": "filesystem" },
    { "src": "/(.*)", "status": 404, "dest": "/404.html" }
  ]
}
JSON
du -sh "$output/static"
