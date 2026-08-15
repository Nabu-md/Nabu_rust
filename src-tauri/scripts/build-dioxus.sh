#!/bin/bash
# Generate the build-time Tailwind stylesheet, then build the Dioxus frontend
# for release (output goes to ../dist for Tauri consumption).
# Also build and stage the native messaging host binary so Tauri's resource
# bundler includes it in Contents/Resources/native-messaging-host.
set -e

# Resolve project root relative to this script (works regardless of cwd).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TAURI_DIR="$PROJECT_ROOT/src-tauri"

npm run css:build
cargo dioxus build --platform web --release --out-dir ../dist

# Build the native messaging host and stage it in src-tauri/ so that
# tauri.bundle.resources ("native-messaging-host") picks it up.
# Cargo builds to the src-tauri standalone workspace's target directory.
cargo build --bin native-messaging-host --release --manifest-path "$TAURI_DIR/Cargo.toml"
cp "$TAURI_DIR/target/release/native-messaging-host" "$TAURI_DIR/native-messaging-host"
chmod +x "$TAURI_DIR/native-messaging-host"
