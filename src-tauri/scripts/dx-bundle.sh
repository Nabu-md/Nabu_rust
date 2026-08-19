#!/bin/bash
# Build the Dioxus frontend via `dx bundle`, then inject the Tailwind CSS
# stylesheet into the generated index.html.
#
# This replaces the manual wasm-build + wasm-bindgen pipeline in
# build-dioxus.sh. The old script is kept as a fallback.
set -e

# Resolve project root relative to this script.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
UI_DIR="$PROJECT_ROOT/crates/nabu-ui"
TAILWIND_SRC="$PROJECT_ROOT/generated/tailwind.css"
DX_OUT="$UI_DIR/web/public"

# 1. Build the Tailwind stylesheet (src/styles/app.css -> generated/tailwind.css).
(cd "$PROJECT_ROOT" && npm run css:build)

# 2. Bundle the Dioxus frontend via the official CLI.
cd "$UI_DIR"
dx bundle --release

# 3. Copy the generated Tailwind CSS into the dx output and inject the
#    <link> tag into the generated index.html (dx does not know about our
#    custom stylesheet).
mkdir -p "$DX_OUT/generated"
cp "$TAILWIND_SRC" "$DX_OUT/generated/tailwind.css"

# Inject CSS link right after <title>Nabu</title>
sed -i.bak 's|<title>Nabu</title>|<title>Nabu</title>\n    <link rel="stylesheet" href="generated/tailwind.css" />|' "$DX_OUT/index.html"
rm -f "$DX_OUT/index.html.bak"

echo "[dx-bundle] Frontend built and styled. Output: $DX_OUT"
