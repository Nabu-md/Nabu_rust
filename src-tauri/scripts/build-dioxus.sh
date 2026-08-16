#!/bin/bash
# Generate the build-time Tailwind stylesheet, then build the Dioxus frontend
# for release (output goes to ../dist for Tauri consumption).
set -e

# Resolve project root relative to this script (works regardless of cwd).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TAURI_DIR="$PROJECT_ROOT/src-tauri"

npm run css:build
cargo dioxus build --platform web --release --out-dir ../dist
