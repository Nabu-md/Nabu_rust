#!/bin/bash
# Generate the build-time Tailwind stylesheet, then build the Dioxus frontend
# for release. The web bundle is emitted to ../dist for Tauri consumption.
#
# NOTE: this uses `cargo build --target wasm32-unknown-unknown` + `wasm-bindgen`
# directly instead of `cargo dioxus build`, so it has no dependency on the
# dioxus-cli toolchain (whose transitive git2/auth-git2 tree does not compile
# cleanly against current rustc).
set -e

# Resolve project root relative to this script (works regardless of cwd).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TAURI_DIR="$PROJECT_ROOT/src-tauri"
UI_DIR="$PROJECT_ROOT/crates/nabu-ui"
OUT_DIR="$PROJECT_ROOT/dist"

# 1. Build the Tailwind stylesheet (src/styles/app.css -> generated/tailwind.css).
npm run css:build

# 2. Compile the Dioxus frontend to wasm32 (cdylib, wasm-bindgen entry).
(cd "$UI_DIR" && cargo build --target wasm32-unknown-unknown --release)

# 3. Generate the ES-module wasm bindings into the output directory.
mkdir -p "$OUT_DIR"
wasm-bindgen \
  "$UI_DIR/target/wasm32-unknown-unknown/release/nabu_ui.wasm" \
  --out-dir "$OUT_DIR" \
  --target web

# 4. Copy the static Tailwind stylesheet where index.html references it.
mkdir -p "$OUT_DIR/generated"
cp "$PROJECT_ROOT/generated/tailwind.css" "$OUT_DIR/generated/tailwind.css"

# 5. Emit index.html: the static template plus a module script that boots the
#    wasm bundle (the app self-starts via its #[wasm_bindgen(start)] entry).
cat > "$OUT_DIR/index.html" <<HTML
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Nabu</title>
    <style>
      html,
      body {
        margin: 0;
        padding: 0;
        background-color: #030712;
        color: #f3f4f6;
        overflow: hidden;
      }
      #boot-splash {
        position: fixed;
        inset: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 12px;
        background-color: #030712;
        z-index: 9999;
      }
      #boot-splash .spinner {
        width: 28px;
        height: 28px;
        border: 3px solid rgba(59, 130, 246, 0.25);
        border-top-color: #3b82f6;
        border-radius: 50%;
        animation: nabu-spin 0.8s linear infinite;
      }
      #boot-splash .brand {
        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        font-size: 14px;
        letter-spacing: 0.04em;
        color: #93c5fd;
      }
      @keyframes nabu-spin {
        to {
          transform: rotate(360deg);
        }
      }
    </style>
    <link rel="stylesheet" href="generated/tailwind.css" />
  </head>
  <body class="bg-gray-950 text-gray-100 m-0 p-0 overflow-hidden">
    <div id="boot-splash">
      <div class="spinner"></div>
      <div class="brand">NABU</div>
    </div>
    <script type="module">
      import init from "./nabu_ui.js";
      init();
    </script>
  </body>
</html>
HTML