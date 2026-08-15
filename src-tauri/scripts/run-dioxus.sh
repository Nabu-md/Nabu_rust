#!/bin/bash
# Generate the build-time Tailwind stylesheet, then serve the frontend with
# the Dioxus CLI (`cargo dioxus serve --platform web`).
#
# On first run this will compile dioxus-cli from source, so it can take a
# few minutes.  Once installed, subsequent invocations are fast.
#
# Also builds and stages the native messaging host binary (debug) so it
# is available for local browser-capture testing during development.
set -e

# Resolve project root relative to this script.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TAURI_DIR="$PROJECT_ROOT/src-tauri"

npm run css:build

# Install dioxus-cli if it's not already available.
if ! command -v cargo-dioxus &>/dev/null; then
    cargo install dioxus-cli
fi

# Build and stage the native messaging host (debug) for dev testing.
cargo build --bin native-messaging-host --manifest-path "$TAURI_DIR/Cargo.toml"
cp "$TAURI_DIR/target/debug/native-messaging-host" "$TAURI_DIR/native-messaging-host"
chmod +x "$TAURI_DIR/native-messaging-host"

# `cargo dioxus serve` builds to wasm32-unknown-unknown, runs wasm-bindgen,
# and serves a dev server at http://localhost:8080.
cargo dioxus serve --platform web --port 8080
