//! # Nabu App Shell — Dioxus root component and routing entry
//!
//! Composes all seven context providers, then delegates to [`AppRouter`] for
//! vault-state-aware routing. Once a vault is configured, the
//! [`WorkspaceLayout`] (ribbon, sidebars, tab bar, navbar, view content, and
//! overlay surfaces) takes over.
//!
//! View switching within the workspace is driven by [`NavContext::view_mode`];
//! actual view content is rendered by [`ViewContent`].

use crate::components::contexts::{use_nav, NavContext, use_workspace};
use crate::components::contexts::{
    HistoryProvider, NavProvider, SaveStatusProvider, ThemeProvider, WorkspaceProvider,
};
use crate::components::layout::WorkspaceLayout;
use crate::components::navigation::{
    CommandPalette, QuickSwitcher, ShortcutReference, ViewMode,
    ArchivePage, CalendarPage, Dashboard, HomeScreen, SearchPage, SmartFoldersPage,
};
use crate::components::ui::feedback::{TaskProvider, ToastProvider};
use crate::components::ui::icons::{render_icon, Icon};
use crate::components::ui::notifications::NotificationManager;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

// ── App shell ───────────────────────────────────────────────────

/// The root component function passed to `dioxus::web::launch::launch_cfg`.
#[allow(non_snake_case)]
pub fn App() -> Element {
    rsx! {
        crate::events::EventServiceProvider {
            crate::metrics::MetricsProvider {
                ThemeProvider { initial_theme: "dark".to_string() }
                ToastProvider {
                    TaskProvider {
                        NotificationManager {}
                        HistoryProvider {
                            SaveStatusProvider {
                                WorkspaceProvider {
                                    NavProvider {
                                        CommandPalette {}
                                        QuickSwitcher {}
                                        ShortcutReference {}
                                        div { class: "app", crate::components::app::AppRouter {} }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Vault-state-aware router ─────────────────────────────────────

/// Vault check lifecycle.
#[derive(Clone, Copy, PartialEq, Debug)]
enum VaultCheckState {
    Loading,
    VaultSetup,
    Error,
    MainDashboard,
}

/// Checks vault state on mount and renders either a loading screen, a
/// vault-setup prompt, or the `WorkspaceLayout`.
#[component]
pub fn AppRouter() -> Element {
    let mut vault_state = use_signal(|| VaultCheckState::Loading);
    let mut vault_error = use_signal(|| String::new());
    let mut nav = use_nav();

    use_effect(move || {
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[IPC-FE] use_effect spawned, invoking check_vault_exists"));
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({})).unwrap();
            let result = crate::ipc::tauri_invoke_safe("check_vault_exists", args).await;
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!("[IPC-FE] check_vault_exists resolved: {:?}", result.as_ref().map(|_| "Ok").map_err(|e| format!("Err({})", e.message())))));
            match result {
                Ok(Some(result)) => {
                    match serde_wasm_bindgen::from_value::<Option<String>>(result) {
                        Ok(Some(path)) if !path.is_empty() => {
                            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!("[IPC-FE] match Ok(Some(path)): vault={}", path)));
                            vault_state.set(VaultCheckState::MainDashboard);
                            let name = path
                                .rsplit('/')
                                .next()
                                .filter(|n| !n.is_empty())
                                .unwrap_or("Vault")
                                .to_string();
                            nav.vault_name.set(name);
                        }
                        Ok(_) => {
                            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[IPC-FE] match Ok(_): no vault path → VaultSetup"));
                            vault_state.set(VaultCheckState::VaultSetup);
                        }
                        Err(e) => {
                            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!("[IPC-FE] match Err(e): {}", e)));
                            vault_state.set(VaultCheckState::Error);
                            vault_error
                                .set("Unexpected response from check_vault_exists".into());
                        }
                    }
                }
                Ok(None) => {
                    // check_vault_exists resolved null → no valid vault is
                    // configured. That is the normal first-run state: launch
                    // the setup wizard. (A rejection is the Error case below.)
                    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str("[IPC-FE] Ok(None): no vault → VaultSetup"));
                    vault_state.set(VaultCheckState::VaultSetup);
                }
                Err(e) => {
                    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!("[IPC-FE] Err(e): {} → Error state", e.message())));
                    vault_state.set(VaultCheckState::Error);
                    vault_error
                        .set("Failed to contact Tauri backend (check_vault_exists)".into());
                }
            }
        });
    });

    let state = *vault_state.read();
    match state {
        VaultCheckState::Loading => rsx! {
            div { class: "flex h-dvh w-dvw items-center justify-center bg-gray-950 text-gray-100",
                div { class: "flex flex-col items-center gap-4",
                    div { class: "w-6 h-6 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" }
                    div { "Opening Nabu..." }
                }
            }
        },
        VaultCheckState::Error => rsx! {
            div { class: "flex h-dvh w-dvw items-center justify-center bg-gray-950 text-red-300",
                div { "{vault_error.read()}" }
            }
        },
        VaultCheckState::VaultSetup => rsx! {
            crate::components::vault_setup_wizard::VaultSetupWizard {
                on_vault_selected: move |path: String| {
                    let name = path
                        .rsplit('/')
                        .next()
                        .filter(|n| !n.is_empty())
                        .unwrap_or("Vault")
                        .to_string();
                    *nav.vault_name.write_unchecked() = name;
                    *vault_state.write_unchecked() = VaultCheckState::MainDashboard;
                }
            }
        },
        VaultCheckState::MainDashboard => rsx! {
            WorkspaceLayout {}
        },
    }
}

// ── View content (switching) ─────────────────────────────────────

/// Switches views based on [`NavContext::view_mode`].
#[component]
pub fn ViewContent() -> Element {
    let nav: NavContext = use_nav();
    let mode = *nav.view_mode.read();

    match mode {
        ViewMode::Dashboard => rsx! { Dashboard {} },
        ViewMode::Editor => {
            let ws = use_workspace();
            if ws.active_path.read().is_some() {
                rsx! {
                    div { class: "max-w-4xl mx-auto h-full",
                        crate::components::note_editor::NoteEditor {}
                    }
                }
            } else {
                rsx! { HomeScreen {} }
            }
        }
        ViewMode::Graph => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::graph_view::GraphView {}
            }
        },
        ViewMode::Search => rsx! { SearchPage {} },
        ViewMode::Settings => rsx! {
            div { class: "max-w-4xl mx-auto h-full",
                crate::components::settings::settings_panel::SettingsPanel {}
            }
        },
        ViewMode::Inbox => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::inbox::Inbox {}
            }
        },
        ViewMode::ReadingQueue => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::reading_queue::ReadingQueue {}
            }
        },
        ViewMode::Templates => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::template_editor::TemplateEditor {}
            }
        },
        ViewMode::Trash => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::trash::Trash {}
            }
        },
        ViewMode::History => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::recovery::version_history::VersionHistory {}
            }
        },
        ViewMode::Recovery => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::recovery::recovery_manager::RecoveryManager {}
            }
        },
        ViewMode::Calendar => rsx! { CalendarPage {} },
        ViewMode::Archive => rsx! { ArchivePage {} },
        ViewMode::SmartFolders => rsx! { SmartFoldersPage {} },
        ViewMode::Canvas => rsx! {
            div { class: "w-full h-full",
                crate::components::canvas::CanvasView {}
            }
        },
        ViewMode::Reader => rsx! {
            div { class: "max-w-4xl mx-auto h-full",
                crate::components::shipped::ReaderView {}
            }
        },
        ViewMode::Comparison => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::shipped::ComparisonView {}
            }
        },
        ViewMode::Statistics => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::statistics::StatisticsView {}
            }
        },
        ViewMode::Activity => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::activity::ActivityProvider {
                    crate::components::activity::ActivityPanel {}
                }
            }
        },
        ViewMode::Streaming => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::streaming::StreamingProvider {
                    crate::components::streaming::StreamingContainer {
                        crate::components::streaming::StreamingContent {}
                    }
                }
            }
        },
        ViewMode::Chat => rsx! {
            div { class: "max-w-7xl mx-auto h-full",
                crate::components::chat::ChatView {}
            }
        },
    }
}
