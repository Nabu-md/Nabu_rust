//! # ChatView — ACP agent chat surface
//!
//! Renders the live streaming conversation from an ACP agent session.
//! Uses the existing [`StreamingProvider`](crate::components::streaming::StreamingProvider)
//! for token rendering and bridges user input through Tauri IPC commands
//! (`acp_connect`, `acp_send_message`, `acp_cancel`, `acp_disconnect`).

use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use serde::Deserialize;
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;

use crate::components::streaming::use_streaming;
use crate::components::ui::feedback::use_toast;
use crate::ipc::tauri_invoke;

#[derive(Debug, Clone, Deserialize)]
struct AcpConnectResult {
    thread_id: String,
    session_id: String,
    protocol_version: u16,
}

/// State for the chat view — tracks connection status and the active thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChatState {
    Disconnected,
    Connecting,
    Connected,
}

/// Root state for the chat view.
#[derive(Clone, Copy)]
pub struct ChatContext {
    pub state: Signal<ChatState>,
    pub thread_id: Signal<Option<Uuid>>,
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
    let streaming = use_streaming();
    let mut chat = ChatContext {
        state: use_signal(|| ChatState::Disconnected),
        thread_id: use_signal(|| None),
    };
    provide_context(chat);

    let mut input_value = use_signal(|| String::new());
    let mut agent_command = use_signal(|| String::new());
    let is_streaming = streaming.has_active();
    let btn_class = if is_streaming {
        "px-4 py-2 text-sm font-medium text-white bg-red-600 rounded-lg hover:bg-red-700"
    } else {
        "px-4 py-2 text-sm font-medium text-white bg-accent rounded-lg hover:bg-accent-hover"
    };

    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "flex-1 overflow-y-auto",
                crate::components::streaming::StreamingProvider {
                    crate::components::streaming::StreamingContainer {
                        crate::components::streaming::StreamingContent { class: "pb-4" }
                    }
                }
            }

            if *chat.state.read() == ChatState::Disconnected {
                div { class: "p-4 border-t border-border bg-surface/50",
                    div { class: "max-w-3xl mx-auto space-y-3",
                        input {
                            class: "w-full px-3 py-2 text-sm border border-border rounded-lg bg-surface focus:outline-none focus:ring-2 focus:ring-accent",
                            placeholder: "Agent command (e.g. \"codex\")",
                            value: "{agent_command}",
                            oninput: move |ev: FormEvent| agent_command.set(ev.value()),
                        }
                        button {
                            class: "px-4 py-2 text-sm font-medium text-white bg-accent rounded-lg hover:bg-accent-hover disabled:opacity-50",
                            disabled: *chat.state.read() == ChatState::Connecting,
                            onclick: move |_| {
                                let cmd = agent_command.read().clone();
                                if cmd.is_empty() {
                                    toasts.error("Agent command required", "Enter the agent command to spawn.");
                                    return;
                                }
                                let chat_clone = chat;
                                let toasts_clone = toasts;
                                spawn_local(async move {
                                    connect_to_agent(&cmd, chat_clone, toasts_clone).await;
                                });
                            },
                            if *chat.state.read() == ChatState::Connecting { "Connecting…" } else { "Connect to Agent" }
                        }
                    }
                }
            }

            if *chat.state.read() == ChatState::Connected {
                div { class: "p-4 border-t border-border bg-surface/50",
                    div { class: "max-w-3xl mx-auto flex gap-3",
                        textarea {
                            class: "flex-1 px-3 py-2 text-sm border border-border rounded-lg bg-surface focus:outline-none focus:ring-2 focus:ring-accent resize-none",
                            placeholder: "Type your message…",
                            rows: "3",
                            value: "{input_value}",
                            oninput: move |ev: FormEvent| input_value.set(ev.value()),
                            onkeydown: move |ev: KeyboardEvent| {
                                let web = ev.as_web_event();
                                if web.key() == "Enter" && !web.shift_key() {
                                    web.prevent_default();
                                    let msg = input_value.read().clone();
                                    if msg.trim().is_empty() {
                                        return;
                                    }
                                    let chat_clone = chat;
                                    let toasts_clone = toasts;
                                    input_value.set(String::new());
                                    spawn_local(async move {
                                        send_chat_message(&msg, chat_clone, toasts_clone).await;
                                    });
                                }
                            },
                        }
                        if is_streaming {
                            button {
                                class: "{btn_class}",
                                onclick: move |_| {
                                    let chat_clone = chat;
                                    let toasts_clone = toasts;
                                    spawn_local(async move {
                                        cancel_chat(chat_clone, toasts_clone).await;
                                    });
                                },
                                "Cancel"
                            }
                        } else {
                            button {
                                class: "{btn_class}",
                                onclick: move |_| {
                                    let msg = input_value.read().clone();
                                    if msg.trim().is_empty() {
                                        return;
                                    }
                                    let chat_clone = chat;
                                    let toasts_clone = toasts;
                                    input_value.set(String::new());
                                    spawn_local(async move {
                                        send_chat_message(&msg, chat_clone, toasts_clone).await;
                                    });
                                },
                                "Send"
                            }
                        }
                        button {
                            class: "px-4 py-2 text-sm font-medium text-white bg-surface/50 border border-border rounded-lg hover:bg-surface/80",
                            onclick: move |_| {
                                let chat_clone = chat;
                                let toasts_clone = toasts;
                                spawn_local(async move {
                                    disconnect_chat(chat_clone, toasts_clone).await;
                                });
                            },
                            "Disconnect"
                        }
                    }
                }
            }
        }
    }
}

/// Spawns the agent process and connects via `acp_connect`.
async fn connect_to_agent(
    cmd: &str,
    mut chat: ChatContext,
    toasts: crate::components::ui::feedback::ToastContext,
) {
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
                toasts.success(
                    "Connected",
                    format!("Connected to ACP agent (session: {})", connect_result.session_id),
                );
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
    message: &str,
    chat: ChatContext,
    toasts: crate::components::ui::feedback::ToastContext,
) {
    let thread_id = match *chat.thread_id.read() {
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

/// Cancels the current streaming response via `acp_cancel`.
async fn cancel_chat(
    mut chat: ChatContext,
    toasts: crate::components::ui::feedback::ToastContext,
) {
    let thread_id = match *chat.thread_id.read() {
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
        Ok(_) => {
            toasts.info("Cancelled", "Streaming response cancelled.");
        }
        Err(e) => {
            toasts.error("Cancel failed", e.message());
        }
    }
}

/// Disconnects from the ACP agent via `acp_disconnect`.
async fn disconnect_chat(
    mut chat: ChatContext,
    toasts: crate::components::ui::feedback::ToastContext,
) {
    chat.state.set(ChatState::Connecting);

    match tauri_invoke("acp_disconnect", serde_wasm_bindgen::to_value(&serde_json::Value::Null).unwrap()).await {
        Ok(_) => {
            chat.state.set(ChatState::Disconnected);
            chat.thread_id.set(None);
            toasts.success("Disconnected", "Disconnected from ACP agent.");
        }
        Err(e) => {
            toasts.error("Disconnect failed", e.message());
            chat.state.set(ChatState::Disconnected);
            chat.thread_id.set(None);
        }
    }
}
