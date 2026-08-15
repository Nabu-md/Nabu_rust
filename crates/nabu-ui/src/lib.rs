//! # Nabu UI — Dioxus frontend crate
//!
//! Entry point for the Nabu knowledge management app's WebAssembly frontend.
//! The WASM bundle is loaded inside a Tauri webview; native integration is
//! handled entirely through the IPC abstraction in [`crate::ipc`].

#![allow(unused_extern_crates)]

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

// ── Shadow of the upstream crate (Phase 1B-2 IPC error handling) ──
//
// Phase 1B-2 changed `tauri_invoke` to return `Result<JsValue, IpcError>`.
// Existing call sites pass these values to the upstream crate's
// `from_value`, which expects a bare `JsValue`.
//
// Strategy: alias the *current crate* as the upstream crate name via
// `extern crate self`.  This takes precedence over the extern prelude entry
// (the external crate from Cargo.toml), so from every module the upstream
// crate's name resolves to `crate::foo`.  We then provide:
//   * a re-export of `Error` and our custom `to_value`, and
//   * a widened `from_value` that accepts `impl IntoJsValue` — so
//     `<crate>::from_value::<T>(result)` works whether `result` is
//     `JsValue`, `Result<JsValue, IpcError>`, or `Option<JsValue>`
//     (including in `inbox.rs`, which must not be modified).
//
// `extern crate self` takes precedence over the extern prelude entry.
extern crate self as serde_wasm_bindgen;

// The upstream `to_value`/`from_value` are reimplemented here using
// `serde_json` + `js_sys::JSON` because `extern crate self` overrides the
// extern crate name, preventing direct access to the upstream crate.
// The behavior is equivalent for all types used in this codebase
// (serde_json::Value, String, Vec<T>, Option<T>, and simple structs).
//
// On non-wasm targets, `js_sys::JSON` panics, so we bridge via
// `JsValue::from_str` / `JsValue::as_string` — the JSON round-trip is
// identical to the wasm path, just without JS engine involvement.
pub type Error = serde_json::Error;

/// Parse a JSON string into a `JsValue`.
#[cfg(target_arch = "wasm32")]
fn js_json_parse(json_str: &str) -> Result<wasm_bindgen::JsValue, Error> {
    js_sys::JSON::parse(json_str).map_err(|e| {
        let msg = e.as_string().unwrap_or_else(|| "JSON parse error".to_string());
        serde::de::Error::custom(msg)
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn js_json_parse(json_str: &str) -> Result<wasm_bindgen::JsValue, Error> {
    Ok(wasm_bindgen::JsValue::from_str(json_str))
}

/// Serializes a Rust value to a `JsValue` via JSON round-trip.
pub fn to_value<T: serde::Serialize>(val: &T) -> Result<wasm_bindgen::JsValue, Error> {
    let json_str = serde_json::to_string(val)?;
    js_json_parse(&json_str)
}

/// Serializes a `JsValue` back to a JSON string.
#[cfg(target_arch = "wasm32")]
fn js_json_stringify(jsval: &wasm_bindgen::JsValue) -> Result<String, Error> {
    let json = js_sys::JSON::stringify(jsval).map_err(|e| {
        let msg = e.as_string().unwrap_or_else(|| "JSON stringify error".to_string());
        serde::de::Error::custom(msg)
    })?;
    Ok(json.as_string().unwrap_or_else(|| "null".to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
fn js_json_stringify(jsval: &wasm_bindgen::JsValue) -> Result<String, Error> {
    Ok(jsval.as_string().unwrap_or_else(|| "null".to_string()))
}

/// Widened `from_value` that accepts `impl [crate::ipc::IntoJsValue]`.
///
/// Delegates to JSON round-tripping.  When the source is a rejected IPC
/// (`Err`) or an absent value (`None`), a deserialization error is returned
/// — preserving the IPC failure so callers can detect it via `.is_ok()` /
/// `.ok()` / `unwrap_or_default()` (including `inbox.rs`).
pub fn from_value<T: serde::de::DeserializeOwned>(
    value: impl crate::ipc::IntoJsValue,
) -> Result<T, Error> {
    match value.into_js_value() {
        Some(jsval) => {
            let json_str = js_json_stringify(&jsval)?;
            serde_json::from_str(&json_str)
        }
        None => Err(serde::de::Error::custom("IPC request failed")),
    }
}

pub mod components;
pub mod events;
pub mod history;
pub mod ipc;
pub mod metrics;
pub mod models;

/// Dioxus launch entry point — called by wasm-bindgen on WASM module init.
/// The boot splash in `index.html` paints instantly while the bundle loads;
/// [`remove_boot_splash`] removes it once the app is about to mount.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    remove_boot_splash();
    dioxus::web::launch::launch_cfg(
        components::app::App,
        dioxus::web::Config::default(),
    );
}

/// Removes the static boot splash element from `index.html` once the app is
/// about to mount.  The splash (dark background + spinner) paints instantly on
/// launch so the window never shows a white flash while the wasm loads.
fn remove_boot_splash() {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(splash) = document.get_element_by_id("boot-splash") {
                splash.remove();
            }
        }
    }
}

// ── Theme context ──────────────────────────────────────────────────

/// Shared theme context.  The `data-theme` attribute on the document root
/// element drives the CSS palette (dark / light / system) via the design
/// tokens in `src/styles/app.css`.
#[derive(Clone, Copy)]
pub struct ThemeContext {
    pub theme: Signal<String>,
}

pub fn provide_theme(initial_theme: String) {
    let theme = use_signal(|| initial_theme);
    let sync_ready = use_signal(|| false);

    provide_context(ThemeContext { theme });

    spawn_local({
        let mut theme = theme;
        let mut sync_ready = sync_ready;
        async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({"key": "theme"}))
                .unwrap_or(JsValue::NULL);
            let result = crate::ipc::tauri_invoke("settings_get", args).await;
            let mut resolved: Option<String> = None;
            if let Ok(saved) = serde_wasm_bindgen::from_value::<String>(result) {
                if !saved.trim().is_empty() {
                    resolved = Some(saved);
                }
            }
            if resolved.is_none() {
                let empty_args = serde_wasm_bindgen::to_value(&serde_json::json!({}))
                    .unwrap_or(JsValue::NULL);
                let settings = crate::ipc::tauri_invoke("get_settings", empty_args).await;
                if let Ok(parsed) = serde_wasm_bindgen::from_value::<SettingsSnapshot>(settings) {
                    if let Some(t) = parsed.theme {
                        if !t.trim().is_empty() {
                            resolved = Some(t);
                        }
                    }
                }
            }
            if let Some(resolved) = resolved {
                theme.set(resolved);
            }
            sync_ready.set(true);
        }
    });

    use_effect(move || {
        let current_theme = theme.read();
        apply_theme_to_document(&current_theme);
        drop(current_theme);

        if !*sync_ready.read() {
            return;
        }

        let theme_val = theme.read().clone();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "key": "theme",
                "value": theme_val,
            })).unwrap_or(JsValue::NULL);
            if let Err(e) = crate::ipc::tauri_invoke("settings_set", args).await {
                web_sys::console::warn_1(&JsValue::from_str(&format!(
                    "Failed to persist theme setting: {}",
                    e
                )));
            }
        });
    });
}

#[derive(serde::Deserialize)]
struct SettingsSnapshot {
    theme: Option<String>,
}

fn apply_theme_to_document(theme: &str) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(root) = document.document_element() {
                let _ = match theme {
                    "dark" => root.set_attribute("data-theme", "dark"),
                    "light" => root.set_attribute("data-theme", "light"),
                    _ => root.remove_attribute("data-theme"),
                };
            }
        }
    }
}

pub fn use_theme() -> ThemeContext {
    use_context::<ThemeContext>()
}

// ── Re-exports ────────────────────────────────────────────────────

pub use components::contexts::*;
