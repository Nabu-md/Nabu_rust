//! # ACP Session Manager — Orchestrates ACP Agent Sessions in Nabu
//!
//! [`AcpSessionManager`] is the bridge between Nabu's application layer and the
//! Phase 3a/b ACP client (`AcpClient<T: Transport>`).  It owns the agent
//! process lifecycle and routes `SessionUpdate` notifications into Nabu's
//! existing [`StreamingPipeline`] so that token chunks flow through the EventBus
//! to the frontend's `StreamingProvider` — the same channel used by every other
//! streaming surface in Nabu.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::acp::client::AcpClient;
use crate::acp::transport::StdioTransport;
use crate::acp::types::{ContentBlock, ContentChunk, PromptResponse, SessionUpdate, TextContent};
use crate::agent::handler::NabuAcpHandler;
use crate::conversations::ConversationStore;
use crate::event_bus::{EventBus, PipelineEvent};
use crate::models::conversation::{Message, Role, Thread, Turn, TurnContent};
use crate::streaming::{StreamingPipeline, StreamSessionHandle};
use crate::storage::StorageManager;

/// Configuration for connecting to an ACP agent via stdio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpConnectConfig {
    /// The executable path or command name to spawn.
    pub command: String,
    /// Command-line arguments to pass to the agent.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the agent process.
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    /// Environment variables for the agent process.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Agent display name (appears in the UI).
    #[serde(default)]
    pub agent_name: Option<String>,
}

/// The concrete transport type used for stdio-based ACP connections.
pub type StdioTransportType = StdioTransport<
    tokio::io::BufReader<tokio::process::ChildStdout>,
    tokio::process::ChildStdin,
>;

/// The concrete client type for stdio-based ACP connections.
pub type AcpClientType = AcpClient<StdioTransportType>;

/// State of a single active ACP session within Nabu.
struct ActiveSession {
    /// The ACP session ID assigned by the agent protocol.
    session_id: String,
    /// The Nabu thread UUID this session persists to.
    thread_id: Uuid,
    /// The streaming handle for token delivery via the EventBus.
    stream_handle: StreamSessionHandle,
    /// Accumulated text chunks grouped by `message_id`.
    accumulated_content: Arc<tokio::sync::Mutex<HashMap<Option<String>, String>>>,
    /// Human-readable agent name for the UI.
    agent_name: Option<String>,
    /// The ACP client — stored here so subsequent prompt/cancel calls can
    /// reuse it.  All post-`initialize` methods take `&self`.
    client: AcpClientType,
}

/// Error returned by `AcpSessionManager` operations.
#[derive(Debug, thiserror::Error)]
pub enum AcpSessionError {
    #[error("No active session — call connect() first")]
    NotConnected,
    #[error("Failed to spawn agent process: {0}")]
    SpawnFailed(String),
    #[error("ACP protocol error: {0}")]
    Acp(#[from] crate::acp::error::AcpError),
    #[error("Stream error: {0}")]
    Stream(#[from] crate::streaming::errors::StreamManagerError),
}

/// The manager owns the live ACP session and bridges updates into Nabu's
/// streaming + conversation subsystems.
pub struct AcpSessionManager {
    /// Streaming pipeline — the single backend-to-frontend streaming channel.
    /// Wrapped in `Arc` so it can be moved into the `on_update` callback closure.
    /// Holds an internal reference to the EventBus.
    pipeline: Arc<StreamingPipeline>,
    /// Storage manager — used by `NabuAcpHandler` for file-system requests.
    storage: Arc<StorageManager>,
    /// Conversation store — persists thread/message/turn history.
    conversation_store: Arc<ConversationStore>,
    /// Active sessions, keyed by the Nabu thread UUID.
    sessions: Arc<tokio::sync::RwLock<HashMap<Uuid, ActiveSession>>>,
}

impl AcpSessionManager {
    /// Creates a new manager with references to the shared subsystem services.
    pub fn new(
        event_bus: Arc<EventBus<PipelineEvent>>,
        storage: Arc<StorageManager>,
        conversation_store: Arc<ConversationStore>,
    ) -> Self {
        let pipeline = Arc::new(StreamingPipeline::new(event_bus.clone()));
        Self {
            pipeline,
            storage,
            conversation_store,
            sessions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Connects to an ACP agent: spawns the process, wires stdio into a
    /// `StdioTransport`, initialises the ACP protocol, creates a new session,
    /// and starts a `StreamingPipeline` stream for token delivery.
    pub async fn connect(
        &self,
        config: &AcpConnectConfig,
    ) -> Result<(Uuid, String), AcpSessionError> {
        let thread_id = Uuid::new_v4();

        let mut child = spawn_agent_process(config)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AcpSessionError::SpawnFailed("agent process has no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AcpSessionError::SpawnFailed("agent process has no stdout".into()))?;
        let _stderr = child.stderr.take();

        let transport = StdioTransport::new(tokio::io::BufReader::new(stdout), stdin);
        let handler: Arc<dyn crate::acp::handler::AcpClientHandler> =
            Arc::new(NabuAcpHandler::new(self.storage.clone()));
        let mut client = AcpClient::new(transport, handler);

        let stream_handle = self.pipeline.start_stream(
            Some(thread_id),
            None,
            config.agent_name.clone(),
        )?;

        let pipeline = self.pipeline.clone();
        let stream_handle_cb = stream_handle.clone();
        let accumulated = Arc::new(tokio::sync::Mutex::new(HashMap::<
            Option<String>,
            String,
        >::new()));

        let accumulated_cb = accumulated.clone();
        let store_cb = self.conversation_store.clone();
        let thread_id_cb = thread_id;

        client.on_update(move |_sid: String, update: SessionUpdate| {
            let pipeline = pipeline.clone();
            let handle = stream_handle_cb.clone();
            let accumulated = accumulated_cb.clone();
            let store = store_cb.clone();
            let thread_id = thread_id_cb;

            async move {
                match update {
                    SessionUpdate::AgentMessageChunk(chunk) => {
                        publish_content_chunk(&pipeline, &handle, &chunk, &accumulated).await;
                    }
                    SessionUpdate::UserMessageChunk(chunk) => {
                        publish_content_chunk(&pipeline, &handle, &chunk, &accumulated).await;
                    }
                    SessionUpdate::AgentThoughtChunk(chunk) => {
                        publish_content_chunk(&pipeline, &handle, &chunk, &accumulated).await;
                    }
                    SessionUpdate::ToolCall(tc) => {
                        let label = format!("[{}] {}", tc.title, tc.tool_call_id);
                        let _ = pipeline.publish_token(&handle, label);
                    }
                    SessionUpdate::ToolCallUpdate(tcu) => {
                        if let Some(ref status) = tcu.status {
                            let label = format!("[{}: {:?}]", tcu.tool_call_id, status);
                            let _ = pipeline.publish_token(&handle, label);
                        }
                    }
                    _ => {}
                }
            }
        });

        let init_response = client
            .initialize(
                Some(crate::acp::types::Implementation {
                    name: "nabu".to_string(),
                    title: Some("Nabu Knowledge Assistant".to_string()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    _meta: None,
                }),
                Some(crate::acp::types::ClientCapabilities {
                    fs: Some(crate::acp::types::FileSystemCapabilities {
                        read_text_file: true,
                        write_text_file: true,
                        _meta: None,
                    }),
                    terminal: false,
                    session: None,
                    _meta: None,
                }),
            )
            .await?;
        let _ = init_response;

        let resolved_wd = config
            .working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let session_id = client
            .new_session(
                &resolved_wd.to_string_lossy(),
                vec![],
                vec![],
            )
            .await?;

        let thread = Thread::with_id(thread_id)
            .with_title(config.agent_name.clone().unwrap_or("ACP Session".to_string()));
        let _ = self.conversation_store.save(&thread);

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                thread_id,
                ActiveSession {
                    session_id: session_id.clone(),
                    thread_id,
                    stream_handle,
                    accumulated_content: accumulated,
                    agent_name: config.agent_name.clone(),
                    client,
                },
            );
        }

        tracing::info!(
            thread_id = %thread_id,
            session_id = %session_id,
            "ACP session connected"
        );

        Ok((thread_id, session_id))
    }

    /// Sends a user message to the active ACP session.
    pub async fn send_message(
        &self,
        thread_id: Uuid,
        message: String,
    ) -> Result<(), AcpSessionError> {
        let (session_id, prompt) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&thread_id)
                .ok_or(AcpSessionError::NotConnected)?;
            (
                session.session_id.clone(),
                build_prompt(&message),
            )
        };

        // Persist the user message.
        persist_user_message(&self.conversation_store, thread_id, &message).await;

        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&thread_id)
            .ok_or(AcpSessionError::NotConnected)?;
        let _response: PromptResponse = session.client.session_prompt(&session_id, prompt).await?;

        Ok(())
    }

    /// Cancels the active prompt turn for the given session.
    pub async fn cancel(&self, thread_id: Uuid) -> Result<(), AcpSessionError> {
        let (session_id, handle) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(&thread_id)
                .ok_or(AcpSessionError::NotConnected)?;
            (session.session_id.clone(), session.stream_handle.clone())
        };

        self.pipeline.cancel_stream(&handle, "user cancelled")?;

        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&thread_id)
            .ok_or(AcpSessionError::NotConnected)?;
        session.client.session_cancel(&session_id).await?;

        Ok(())
    }

    /// Disconnects from an ACP session, closing the protocol session and
    /// terminating the agent process.
    pub async fn disconnect(&self, thread_id: Uuid) -> Result<(), AcpSessionError> {
        let mut session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&thread_id)
        }
        .ok_or(AcpSessionError::NotConnected)?;

        let session_id = session.session_id.clone();
        let _ = self
            .pipeline
            .cancel_stream(&session.stream_handle, "session disconnected");

        let _ = session.client.close_session(&session_id).await;
        let _ = session.client.shutdown().await;

        Ok(())
    }

    /// Lists the session IDs of all active ACP sessions.
    pub async fn list_sessions(&self) -> Result<Vec<String>, AcpSessionError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.values().map(|s| s.session_id.clone()).collect())
    }

    pub fn streaming_pipeline(&self) -> &StreamingPipeline {
        &self.pipeline
    }
}

fn spawn_agent_process(config: &AcpConnectConfig) -> Result<tokio::process::Child, AcpSessionError> {
    let mut cmd = tokio::process::Command::new(&config.command);
    cmd.args(&config.args);
    if !config.env.is_empty() {
        cmd.envs(&config.env);
    }
    if let Some(ref dir) = config.working_dir {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    cmd.spawn()
        .map_err(|e| AcpSessionError::SpawnFailed(e.to_string()))
}

fn build_prompt(message: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent {
        text: message.to_string(),
        annotations: None,
        _meta: None,
    })]
}

/// Publishes a `ContentChunk`'s text as a token to the streaming pipeline,
/// accumulating it for later persistence.
async fn publish_content_chunk(
    pipeline: &StreamingPipeline,
    handle: &StreamSessionHandle,
    chunk: &ContentChunk,
    accumulated: &Arc<tokio::sync::Mutex<HashMap<Option<String>, String>>>,
) {
    let text = match &chunk.content {
        ContentBlock::Text(tc) => tc.text.clone(),
        _ => return,
    };

    {
        let mut map = accumulated.lock().await;
        let entry = map
            .entry(chunk.message_id.clone())
            .or_insert_with(String::new);
        entry.push_str(&text);
    }

    let _ = pipeline.publish_token(handle, text);
}

/// Persists a user message as a `Message` (role: User) on the thread.
async fn persist_user_message(
    store: &ConversationStore,
    thread_id: Uuid,
    message: &str,
) {
    if let Ok(mut thread) = store.load(thread_id) {
        let user_msg = Message::new(Uuid::new_v4(), thread_id)
            .with_role(Role::User)
            .with_turn(Turn::new_anonymous(TurnContent::Text(message.to_string())));
        thread.messages.push(user_msg);
        if let Err(e) = store.save(&thread) {
            tracing::error!(error = %e, "Failed to persist user message to conversation store");
        }
    }
}
