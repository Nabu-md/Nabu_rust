#!/bin/bash
# Install or remove the native messaging host manifest for Chrome/Chromium
# and Firefox.  The manifest points to the Nabu binary bundled inside the
# application package — no manual binary copying is required.
#
# Usage:
#   ./src-tauri/scripts/install-native-messaging.sh install [chrome|firefox] [extension-id]
#   ./src-tauri/scripts/install-native-messaging.sh uninstall chrome
#
# When no browser argument is given, manifests are installed for both Chrome
# and Firefox.

set -e

BROWSER="${2:-both}"
EXTENSION_ID="${3:-}"

# Determine the Nabu application bundle path (platform-specific).
if [[ "$OSTYPE" == "darwin"* ]]; then
    APP_PATH="/Applications/Nabu.app/Contents/Resources/native-messaging-host"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    APP_PATH="/usr/lib/nabu/native-messaging-host"
    # Fallback: alongside the installed app binary
    if [[ ! -f "$APP_PATH" ]]; then
        APP_PATH="$(command -v nabu 2>/dev/null || echo /usr/local/bin/nabu)"
    fi
else
    echo "Unsupported platform: $OSTYPE"
    exit 1
fi

if [[ ! -f "$APP_PATH" ]]; then
    echo "Error: Nabu native messaging host binary not found at $APP_PATH"
    echo "       Ensure Nabu is installed and the bundle includes the host binary."
    exit 1
fi

# Chrome / Chromium use allowed_origins; Firefox uses allowed_extensions.
generate_manifest() {
    local browser="$1"
    local ext_id="$2"
    local manifest_type
    local key_name

    if [[ "$browser" == "firefox" ]]; then
        manifest_type="allowed_extensions"
        ext_id="${ext_id:-nabu-capture@addons.mozilla.org}"
    else
        manifest_type="allowed_origins"
        ext_id="${ext_id:-$ext_id}"
    fi

    cat <<EOF
{
  "name": "com.nabu.capture.host",
  "description": "Nabu native messaging host for browser capture",
  "path": "$APP_PATH",
  "type": "stdio",
  "$manifest_type": ["$ext_id"]
}
EOF
}

install_for_browser() {
    local browser="$1"
    local ext_id="$2"
    local dest_dir

    if [[ "$OSTYPE" == "darwin"* ]]; then
        if [[ "$browser" == "firefox" ]]; then
            dest_dir="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
        else
            dest_dir="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
        fi
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if [[ "$browser" == "firefox" ]]; then
            dest_dir="$HOME/.mozilla/native-messaging-hosts"
        else
            dest_dir="$HOME/.config/google-chrome/NativeMessagingHosts"
        fi
    fi

    mkdir -p "$dest_dir"
    local manifest_file="$dest_dir/com.nabu.capture.host.json"

    if [[ -z "$ext_id" ]]; then
        echo "Error: extension ID is required for $browser."
        echo "  Find it at chrome://extensions/ (Chrome) or about:debugging (Firefox)."
        exit 1
    fi

    generate_manifest "$browser" "$ext_id" > "$manifest_file"
    echo "Installed $browser native messaging manifest:"
    echo "  $manifest_file -> $APP_PATH"
}

uninstall_for_browser() {
    local browser="$1"

    if [[ "$OSTYPE" == "darwin"* ]]; then
        if [[ "$browser" == "firefox" ]]; then
            rm -f "$HOME/Library/Application Support/Mozilla/NativeMessagingHosts/com.nabu.capture.host.json"
        else
            rm -f "$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.nabu.capture.host.json"
        fi
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if [[ "$browser" == "firefox" ]]; then
            rm -f "$HOME/.mozilla/native-messaging-hosts/com.nabu.capture.host.json"
        else
            rm -f "$HOME/.config/google-chrome/NativeMessagingHosts/com.nabu.capture.host.json"
        fi
    fi
    echo "Uninstalled $browser native messaging manifest."
}

ACTION="${1:-install}"

case "$ACTION" in
    install)
        if [[ "$BROWSER" == "both" ]]; then
            install_for_browser "chrome" "$EXTENSION_ID"
            install_for_browser "firefox" "$EXTENSION_ID"
        else
            install_for_browser "$BROWSER" "$EXTENSION_ID"
        fi
        ;;
    uninstall)
        if [[ "$BROWSER" == "both" ]]; then
            uninstall_for_browser "chrome"
            uninstall_for_browser "firefox"
        else
            uninstall_for_browser "$BROWSER"
        fi
        ;;
    *)
        echo "Usage: $0 {install|uninstall} [chrome|firefox|both] [extension-id]"
        exit 1
        ;;
esac
