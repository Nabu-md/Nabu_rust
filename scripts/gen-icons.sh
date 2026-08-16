#!/usr/bin/env bash
# Regenerates the FULL application icon set from the canonical master icon.
#
# Pipeline:
#   1. Build `resources/icon-master.png` (1024x1024) from `nabu logo.png` by
#      placing the artwork at 80% scale on a transparent canvas (~10% padding
#      on each side). The transparent padding is what lets macOS apply its
#      rounded-rect squircle mask in the Dock/Finder. Regenerating from the
#      raw (opaque) logo directly would produce square icons shown unmasked.
#   2. Run `cargo tauri icon` from the master -> `src-tauri/icons/`
#      (icns, ico, PNGs, plus iOS and Android sets).
#   3. Sync derived assets that are also generated from the master:
#      `resources/icon.png` (512).
#
# Run from anywhere; resolves the repo root via its own location.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MASTER="$ROOT/resources/icon-master.png"
SOURCE="$ROOT/nabu logo.png"
SIZE=1024
ART=$((SIZE * 80 / 100))   # artwork side length -> ~10% padding each side

cd "$ROOT"

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: python3 with PIL required to build the master icon" >&2
  exit 1
fi

echo "==> Building master icon (${SIZE}x${SIZE}, art ${ART}px, ~10% transparent padding)"
python3 - "$SOURCE" "$MASTER" "$SIZE" "$ART" <<'PY'
import sys
from PIL import Image

src, dst, size, art = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
artwork = Image.open(src).convert("RGBA").resize((art, art), Image.LANCZOS)
offset = (size - art) // 2
canvas.paste(artwork, (offset, offset))
canvas.save(dst)
print("    wrote", dst)
PY

echo "==> Regenerating src-tauri/icons via \`cargo tauri icon\`"
(cd "$ROOT/src-tauri" && cargo tauri icon "$MASTER" -o icons)

echo "==> Syncing resources/icon.png (512)"
python3 - "$MASTER" "$ROOT/resources/icon.png" 512 <<'PY'
import sys
from PIL import Image
src, dst, size = sys.argv[1], sys.argv[2], int(sys.argv[3])
Image.open(src).convert("RGBA").resize((size, size), Image.LANCZOS).save(dst)
print("    wrote", dst)
PY


echo "Done. Icon set refreshed from resources/icon-master.png."
