//! # Vault Setup Wizard (Dioxus)
//!
//! First-launch entry point rendered by [`AppRouter`](crate::components::app::AppRouter)
//! when no usable vault is configured (`check_vault_exists` returns `None`).
//!
//! The wizard lets the user select an existing directory or create a new one as
//! their vault. On success it drives the `select_vault_dialog` /
//! `create_vault_dialog` IPC commands (which persist `last_vault_path` in the
//! settings store) and then `complete_setup` (which materialises the canonical
//! application context against the chosen vault), after which it notifies the
//! router via `on_vault_selected` so the workspace can mount.

use crate::components::ui::icons::{render_icon, Icon};
use crate::ipc::tauri_invoke;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// First-launch vault selection screen.
///
/// `on_vault_selected` fires with the chosen vault path once the backend
/// context has been initialised against it, so the host router can transition
/// to the main workspace.
#[component]
pub fn VaultSetupWizard(on_vault_selected: Callback<String>) -> Element {
    let loading = use_signal(|| false);
    let error = use_signal(|| None::<String>);

    rsx! {
        div { class: "flex h-dvh w-dvw items-center justify-center bg-gray-950 text-gray-100 p-6 select-none",
            div { class: "max-w-md w-full bg-gray-900 border border-gray-800 rounded-xl p-8 shadow-2xl space-y-6",
                div { class: "text-center space-y-2",
                    div { class: "inline-flex items-center justify-center w-16 h-16 rounded-full bg-blue-600/20 text-blue-400 text-3xl mb-2",
                        {render_icon(Icon::BookOpen, Some("w-8 h-8 text-blue-400"))}
                    }
                    h1 { class: "text-2xl font-bold tracking-tight text-white", "Welcome to Nabu" }
                    p { class: "text-sm text-gray-400",
                        "Select or create a markdown directory to initialize your knowledge vault."
                    }
                }

                {error.read().as_ref().map(|e| rsx! {
                    div {
                        class: "rounded-md bg-red-900/30 border border-red-700/50 text-red-300 px-4 py-3 text-sm",
                        "{e}"
                    }
                })}

                div { class: "space-y-4 pt-2",
                    button {
                        class: "w-full flex items-center gap-3 text-left px-4 py-3 bg-gray-800 border border-gray-800 rounded-lg hover:bg-gray-700 hover:border-gray-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *loading.read(),
                        onclick: move |_: MouseEvent| {
                            pick_vault("select_vault_dialog", loading, error, on_vault_selected)
                        },
                        {render_icon(Icon::FolderOpen, Some("w-5 h-5 text-gray-300"))}
                        div { class: "flex-1",
                            div { class: "text-sm font-semibold text-white", "Select Existing Vault" }
                            div { class: "text-xs text-gray-400", "Open an existing folder with notes" }
                        }
                        {render_icon(Icon::ExternalLink, Some("w-4 h-4 text-gray-500"))}
                    }
                    button {
                        class: "w-full flex items-center gap-3 text-left px-4 py-3 bg-gray-800 border border-gray-800 rounded-lg hover:bg-gray-700 hover:border-gray-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *loading.read(),
                        onclick: move |_: MouseEvent| {
                            pick_vault("create_vault_dialog", loading, error, on_vault_selected)
                        },
                        {render_icon(Icon::Sparkles, Some("w-5 h-5 text-amber-400"))}
                        div { class: "flex-1",
                            div { class: "text-sm font-semibold text-white", "Create New Vault" }
                            div { class: "text-xs text-gray-400", "Initialize a new directory for Nabu" }
                        }
                        {render_icon(Icon::ExternalLink, Some("w-4 h-4 text-gray-500"))}
                    }
                }

                if *loading.read() {
                    div { class: "flex items-center justify-center gap-2 text-xs text-blue-400 pt-2",
                        div { class: "w-3 h-3 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" }
                        span { "Opening system dialog\u{2026}" }
                    }
                }
            }
        }
    }
}

/// Drives a vault-selection IPC command, initialises the backend context against
/// the chosen directory, and notifies the router on success.
///
/// `loading` and `error` are shared signals so both buttons can read/write the
/// same state; they are `Copy` so each closure captures its own handle. The
/// async work runs on the JS event loop via `spawn_local`.
fn pick_vault(
    cmd: &'static str,
    loading: Signal<bool>,
    error: Signal<Option<String>>,
    on_vault_selected: Callback<String>,
) {
    if *loading.read() {
        return;
    }
    *loading.write_unchecked() = true;
    *error.write_unchecked() = None;
    spawn_local(async move {
        let args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();
        match tauri_invoke(cmd, args).await {
            Ok(val) => match serde_wasm_bindgen::from_value::<Option<String>>(val) {
                Ok(Some(path)) if !path.trim().is_empty() => {
                    let setup_args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();
                    match tauri_invoke("complete_setup", setup_args).await {
                        Ok(_) => {
                            *loading.write_unchecked() = false;
                            on_vault_selected.call(path);
                        }
                        Err(e) => {
                            *loading.write_unchecked() = false;
                            *error.write_unchecked() =
                                Some(format!("Failed to initialise vault: {}", e.message()));
                        }
                    }
                }
                _ => {
                    *loading.write_unchecked() = false;
                    *error.write_unchecked() = Some("No directory selected.".to_string());
                }
            },
            Err(e) => {
                *loading.write_unchecked() = false;
                *error.write_unchecked() = Some(format!("{cmd} failed: {}", e.message()));
            }
        }
    });
}
