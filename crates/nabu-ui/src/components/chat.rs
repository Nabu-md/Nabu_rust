//! # ChatView — ACP agent chat surface
//!
//! Renders the live streaming conversation from an ACP agent session.
//! Uses the existing [`StreamingProvider`](crate::components::streaming::StreamingProvider)
//! for token rendering and bridges user input through Tauri IPC commands
//! (`acp_connect`, `acp_send_message`, `acp_cancel`, `acp_disconnect`).

use std::sync::Arc;

use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::navigation::state::ViewMode;
use crate::components::ui::feedback::use_toast;
use crate::events::use_event_service;
use crate::ipc::tauri_invoke;

#[derive(Debug, Clone, Deserialize)]
struct AcpConnectResult {
    thread_id: String,
    session_id: String,
    protocol_version: u16,
}

#[derive(Debug, Clone, Deserialize)]
struct AcpSessionSummary {
    session_id: String,
    thread_id: String,
    agent_name: Option<String>,
}

/// State for the ChatView — tracks connection status and the active thread.
#[derive(Clone, Copy, PartialEq, Debug)]
enum ChatState {
    Disconnected,
    Connecting,
    Connected,
}

/// Root state for the chat view.
#[derive(Clone)]
struct ChatContext {
    state: Signal<ChatState>,
    thread_id: Signal<Option<Uuid>>,
}

/// Retrieves the shared chat context.
pub fn use_chat() -> ChatContext {
    use_context::<ChatContext>()
}

/// The chat view — connects to an ACP agent and renders a streaming conversation.
#[allow(non_snake_case)]
#[component]
pub fn ChatView() -> Element {
    let toasts = use_toast();
    let chat = ChatContext {
        state: use_signal(|| ChatState::Disconnected),
        thread_id: use_signal(|| None),
    };
    provide_context(chat.clone());

    let input_value = use_signal(|| String::new());
    let agent_command = use_signal(|| String::new());

    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "flex-1 overflow-y-auto",
                crate::components::streaming::StreamingProvider {
                    crate::components::streaming::StreamingContainer {
                        crate::components::streaming::StreamingContent { class: "pb-4" }
                    }
                }
            }

            if chat.state.read() == ChatState::Disconnected {
                div { class: "p-4 border-t border-border bg-surface/50",
                    div { class: "max-w-3xl mx-auto space-y-3",
                        div { class: "flex gap-2",
                            input {
                                class: "flex-1 px-3 py-2 text-sm border border-border rounded-lg bg-surface focus:outline-none focus:ring-2 focus:ring-accent",
                                placeholder: "Agent command (e.g. \"codex\")",
                                value: "{agent_command}",
                                oninput: move |e| agent_command.set(e.value.clone()),
                            }
                        }
                        div { class: "flex gap-2",
                            input {
                                class: "flex-1 px-3 py-2 text-sm border border-border rounded-lg bg-surface focus:outline-none focus:ring-2 focus:ring-accent",
                                placeholder: "Agent arguments (space-separated, optional)",
                                value: "{agent_command.clone()}",
                                oninput: move |e| agent_command.set(e.value.clone()),
                            }
                        }
                        button {
                            class: "px-4 py-2 text-sm font-medium text-white bg-accent rounded-lg hover:bg-accent-hover disabled:opacity-50",
                            disabled: matches!(chat.state.read(), ChatState::Connecting),
                            onclick: move |_| {
                                let cmd = agent_command.read().clone();
                                if cmd.is_empty() {
                                    toasts.error("Agent command required", "Enter the agent command to spawn.");
                                    return;
                                }
                                let chat_clone = chat.clone();
                                let toasts_clone = toasts.clone();
                                spawn_local(async move {
                                    connect_to_agent(&cmd, chat_clone, toasts_clone).await;
                                });
                            },
                            if matches!(chat.state.read(), ChatState::Connecting) { "Connecting…" } else { "Connect to Agent" }
                        }
                    }
                }
            }

            if chat.state.read() == ChatState::Connected {
                div { class: "p-4 border-t border-border bg-surface/50",
                    div { class: "max-w-3xl mx-auto",
                        div { class: "flex gap-3",
                            textarea {
                                class: "flex-1 px-3 py-2 text-sm border border-border rounded-lg bg-surface focus:outline-none focus:ring-2 focus:ring-accent resize-none",
                                placeholder: "Type your message…",
                                rows: "3",
                                value: "{input_value}",
                                oninput: move |e| input_value.set(e.value.clone()),
                                onkeydown: move |e| {
                                    if e.key() == "Enter" && !e.shift_key() {
                                        e.prevent_default();
                                        let msg = input_value.read().clone();
                                        if msg.trim().is_empty() {
                                            return;
                                        }
                                        let chat_clone = chat.clone();
                                        let toasts_clone = toasts.clone();
                                        input_value.set(String::new());
                                        spawn_local(async move {
                                            send_chat_message(chat_clone, &msg, toasts_clone).await;
                                        });
                                    }
                                },
                            }
                            button {
                                class: "px-4 py-2 text-sm font-medium text-white bg-accent rounded-lg hover:bg-accent-hover",
                                onclick: move |_| {
                                    let msg = input_value.read().clone();
                                    if msg.trim().is_empty() {
                                        return;
                                    }
                                    let chat_clone = chat.clone();
                                    let toasts_clone = toasts.clone();
                                    input_value.set(String::new());
                                    spawn_local(async move {
                                        send_chat_message(chat_clone, &msg, toasts_clone).await;
                                    });
                                },
                                "Send"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Spawns the agent process and connects via `acp_connect`.
async fn connect_to_agent(cmd: &str, chat: ChatContext, toasts: crate::components::ui::feedback::ToastContext) {
    chat.state.set(ChatState::Connecting);

    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        toasts.error("Invalid command", "Could not parse agent command.");
        chat.state.set(ChatState::Disconnected);
        return;
    }

    let command = parts[0].to_string();
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    let config = serde_json::json!({
        "command": command,
        "args": args,
        "agent_name": "ACP Agent",
    });

    let args_val = serde_wasm_bindgen::to_value(&config).unwrap();

    let result = tauri_invoke("acp_connect", args_val).await;

    match result {
        Ok(val) => {
            if let Ok(connect_result) = serde_wasm_bindgen::from_value::<AcpConnectResult>(val) {
                if let Ok(uuid) = Uuid::parse_str(&connect_result.thread_id) {
                    chat.thread_id.set(Some(uuid));
                }
                chat.state.set(ChatState::Connected);
                toasts.success("Connected", format!("Connected to ACP agent (session: {})", connect_result.session_id));
            }
        }
        Err(e) => {
            toasts.error("Connection failed", e.message());
            chat.state.set(ChatState::Disconnected);
        }
    }
}

/// Sends a message via `acp_send_message`.
async fn send_chat_message(
    chat: ChatContext,
    message: &str,
    toasts: crate::components::ui::feedback::ToastContext,
) {
    let thread_id = match chat.thread_id.read().as_ref() {
        Some(id) => id.to_string(),
        None => {
            toasts.error("Not connected", "No active ACP session.");
            return;
        }
    };

    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
        "thread_id": thread_id,
        "message": message,
    }))
    .unwrap();

    match tauri_invoke("acp_send_message", args).await {
        Ok(_) => {
            // Message sent successfully — tokens will arrive via streaming.
        }
        Err(e) => {
            toasts.error("Message failed", e.message());
        }
    }
}

/// Cancels the current turn via `acp_cancel`.
pub async fn cancel_turn(chat: ChatContext, toasts: crate::components::ui::feedback::ToastContext) {
    let thread_id = match chat.thread_id.read().as_ref() {
        Some(id) => id.to_string(),
        None => {
            toasts.error("Not connected", "No active ACP session.");
            return;
        }
    };

    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
        "thread_id": thread_id,
    }))
    .unwrap();

    match tauri_invoke("acp_cancel", args).await {
        Ok(_) => {}
        Err(e) => {
            toasts.error("Cancel failed", e.message());
        }
    }
}

/// Disconnects from the active session via `acp_disconnect`.
pub async fn disconnect_agent(chat: ChatContext, toasts: crate::components::ui::feedback::ToastContext) {
    let thread_id = match chat.thread_id.read().as_ref() {
        Some(id) => id.to_string(),
        None => return,
    };

    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
        "thread_id": thread_id,
    }))
    .unwrap();

    match tauri_invoke("acp_disconnect", args).await {
        Ok(_) => {
            chat.state.set(ChatState::Disconnected);
            chat.thread_id.set(None);
            toasts.success("Disconnected", "ACP session closed.");
        }
        Err(e) => {
            toasts.error("Disconnect failed", e.message());
        }
    }
}
