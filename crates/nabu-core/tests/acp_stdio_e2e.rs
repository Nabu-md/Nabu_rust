//! # ACP stdio E2E Integration Tests
//!
//! These tests spawn a real ACP agent subprocess (`acp-test-agent` binary)
//! and verify that `crate::acp::client::AcpClient<StdioTransport>` correctly
//! drives it through the full ACP lifecycle over actual stdio pipes.
//!
//! This is the *real* integration test — no mock transports, no in-memory
//! channels.  The agent process sends real JSON-RPC messages over real pipes,
//! and the client reads real `session/update` notifications.

use std::path::PathBuf;
use std::sync::Arc;

use nabu_core::acp::client::AcpClient;
use nabu_core::acp::handler::{AcpClientHandler, NoopClientHandler};
use nabu_core::acp::transport::StdioTransport;
use nabu_core::acp::types::{
    ClientCapabilities, ContentBlock, Implementation, PromptResponse, SessionUpdate, TextContent,
};
use nabu_core::event_bus::{EventBus, PipelineEvent};
use nabu_core::streaming::StreamingPipeline;
use tokio::io::BufReader;
use tokio::process::Command;

/// Path to the `acp-test-agent` binary, set by Cargo during test compilation.
fn test_agent_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_acp-test-agent"))
}

/// Start a test agent subprocess with optional failure flags.
async fn spawn_agent(args: &[&str]) -> tokio::process::Child {
    let mut cmd = Command::new(test_agent_bin());
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    cmd.spawn().expect("failed to spawn test agent")
}

/// Type alias for the stdio transport + client used in tests.
type TestClient = AcpClient<StdioTransport<BufReader<tokio::process::ChildStdout>, tokio::process::ChildStdin>>;

/// Connect an AcpClient to the given child process's stdin/stdout.
fn make_client(child: &mut tokio::process::Child) -> TestClient {
    let stdin = child
        .stdin
        .take()
        .expect("child has no stdin");
    let stdout = child
        .stdout
        .take()
        .expect("child has no stdout");

    let transport = StdioTransport::new(BufReader::new(stdout), stdin);
    let handler: Arc<dyn AcpClientHandler> = Arc::new(NoopClientHandler);
    AcpClient::new(transport, handler)
}

async fn init_client(client: &mut TestClient) {
    let _ = client
        .initialize(
            Some(Implementation {
                name: "nabu-e2e".to_string(),
                title: Some("Nabu E2E Test".to_string()),
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            Some(ClientCapabilities::default()),
        )
        .await
        .expect("initialize");
}

async fn new_session(client: &TestClient) -> String {
    client
        .new_session("/tmp", vec![], vec![])
        .await
        .expect("new_session")
}

// ===========================================================================
// Full lifecycle test: initialize → session/new → session/prompt → session/close
// ===========================================================================

#[tokio::test]
async fn full_lifecycle_stdio() {
    let mut child = spawn_agent(&[]).await;
    let mut client = make_client(&mut child);

    // --- initialize ---
    let init_resp = client
        .initialize(
            Some(Implementation {
                name: "nabu-e2e".to_string(),
                title: Some("Nabu E2E Test".to_string()),
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            Some(ClientCapabilities::default()),
        )
        .await
        .expect("initialize");

    assert_eq!(init_resp.protocol_version, 1);
    assert_eq!(init_resp.agent_info.as_ref().unwrap().name, "acp-test-agent");

    // --- new session ---
    let sid = client
        .new_session(
            std::env::temp_dir().to_string_lossy().as_ref(),
            vec![],
            vec![],
        )
        .await
        .expect("new_session");
    assert_eq!(sid, "test-session-123");

    // --- prompt ---
    let prompt = vec![ContentBlock::Text(TextContent {
        text: "Hello, agent!".to_string(),
        annotations: None,
        _meta: None,
    })];
    let resp = client
        .session_prompt(&sid, prompt)
        .await
        .expect("session_prompt");
    assert_eq!(resp.stop_reason, nabu_core::acp::types::StopReason::EndTurn);

    // --- close ---
    let _ = client.close_session(&sid).await;
    let _ = client.shutdown().await;
}

// ===========================================================================
// Full lifecycle with streaming: verify session/update notifications
// ===========================================================================

#[tokio::test]
async fn session_update_notifications_streamed() {
    let mut child = spawn_agent(&[]).await;
    let mut client = make_client(&mut child);

    // Set up a streaming pipeline and callback
    let bus: Arc<EventBus<PipelineEvent>> = Arc::new(EventBus::new());
    let pipeline = Arc::new(StreamingPipeline::new(bus));
    let stream_handle = pipeline
        .start_stream(Some(uuid::Uuid::new_v4()), None, Some("acp-test-agent".to_string()))
        .expect("start_stream");

    let collected: Arc<tokio::sync::Mutex<Vec<String>>> =
        Arc::new(tokio::sync::Mutex::new(vec![]));
    let collected_cb = collected.clone();
    let handle_cb = stream_handle.clone();

    client.on_update(move |_sid: String, update: SessionUpdate| {
        let collected = collected_cb.clone();
        let handle = handle_cb.clone();
        async move {
            match update {
                SessionUpdate::AgentMessageChunk(chunk) => {
                    if let ContentBlock::Text(tc) = &chunk.content {
                        let mut v = collected.lock().await;
                        v.push(tc.text.clone());
                        let _ = handle.publish_token(tc.text.clone());
                    }
                }
                _ => {}
            }
        }
    });

    // initialize + new_session
    init_client(&mut client).await;
    let sid = new_session(&client).await;

    // prompt — the mock agent sends 3 session/update notifications
    let prompt = vec![ContentBlock::Text(TextContent {
        text: "Hello".to_string(),
        annotations: None,
        _meta: None,
    })];

    let _resp: PromptResponse = client
        .session_prompt(&sid, prompt)
        .await
        .expect("session_prompt");

    // Give the read loop time to process notifications
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let received = collected.lock().await;
    assert!(
        received.len() >= 3,
        "expected at least 3 notification chunks, got {}",
        received.len()
    );
    let combined: String = received.concat();
    assert_eq!(combined, "Hello world!\n");

    let _ = client.close_session(&sid).await;
    let _ = client.shutdown().await;
}

// ===========================================================================
// Process failure: agent exits after init
// ===========================================================================

#[tokio::test]
async fn agent_exit_after_init_returns_error_on_next_request() {
    let mut child = spawn_agent(&["--exit-after-init"]).await;
    let mut client = make_client(&mut child);

    // Initialize should succeed (the agent responds before exiting)
    let _ = client
        .initialize(
            Some(Implementation {
                name: "nabu-e2e".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            Some(ClientCapabilities::default()),
        )
        .await
        .expect("initialize should succeed before exit");

    // Give the agent time to exit
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // new_session should fail because the agent has exited
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.new_session("/tmp", vec![], vec![]),
    )
    .await;

    assert!(result.is_ok(), "should get an error response, not hang");
    assert!(result.unwrap().is_err(), "new_session should fail after agent exit");
}

// ===========================================================================
// Process failure: agent exits during prompt
// ===========================================================================

#[tokio::test]
async fn agent_exit_during_prompt() {
    let mut child = spawn_agent(&["--exit-during-prompt"]).await;
    let mut client = make_client(&mut child);

    init_client(&mut client).await;
    let sid = new_session(&client).await;

    let prompt = vec![ContentBlock::Text(TextContent {
        text: "Hello".to_string(),
        annotations: None,
        _meta: None,
    })];

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.session_prompt(&sid, prompt),
    )
    .await;

    assert!(result.is_ok(), "should get a response, not hang");
    assert!(result.unwrap().is_err(), "prompt should fail when agent exits");
}

// ===========================================================================
// Process failure: malformed JSON response
// ===========================================================================

#[tokio::test]
async fn agent_malformed_response() {
    let mut child = spawn_agent(&["--malformed-response"]).await;
    let mut client = make_client(&mut child);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.initialize(
            Some(Implementation {
                name: "nabu-e2e".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            Some(ClientCapabilities::default()),
        ),
    )
    .await;

    assert!(result.is_ok(), "should get a response, not hang");
    assert!(result.unwrap().is_err(), "initialize should fail with malformed JSON");
}

// ===========================================================================
// Cancellation: client sends session/cancel
// ===========================================================================

#[tokio::test]
async fn cancel_notification_sent() {
    let mut child = spawn_agent(&[]).await;
    let mut client = make_client(&mut child);

    init_client(&mut client).await;
    let sid = new_session(&client).await;

    // Cancel — the mock agent sends a session/cancel notification back
    let result = client.session_cancel(&sid).await;
    assert!(result.is_ok(), "session_cancel: {:?}", result);

    let _ = client.shutdown().await;
}

// ===========================================================================
// Stderr is not treated as protocol data
// ===========================================================================

#[tokio::test]
async fn stderr_noise_does_not_break_protocol() {
    let mut child = spawn_agent(&["--stderr-noise"]).await;
    let mut client = make_client(&mut child);

    let result = client
        .initialize(
            Some(Implementation {
                name: "nabu-e2e".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            Some(ClientCapabilities::default()),
        )
        .await;

    assert!(
        result.is_ok(),
        "initialize should succeed despite stderr noise: {:?}",
        result
    );

    let _ = client.shutdown().await;
}

// ===========================================================================
// Multiple concurrent requests don't deadlock
// ===========================================================================

#[tokio::test]
async fn concurrent_requests_no_deadlock() {
    let mut child = spawn_agent(&[]).await;
    let mut client = make_client(&mut child);

    init_client(&mut client).await;
    let sid = new_session(&client).await;

    // Send two prompts concurrently — this would deadlock with the old
    // implementation because the read loop holds the state lock.
    let prompt1 = client.session_prompt(&sid, vec![ContentBlock::Text(TextContent {
        text: "first".to_string(),
        annotations: None,
        _meta: None,
    })]);
    let prompt2 = client.session_prompt(&sid, vec![ContentBlock::Text(TextContent {
        text: "second".to_string(),
        annotations: None,
        _meta: None,
    })]);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        async {
            tokio::join!(prompt1, prompt2)
        },
    )
    .await;

    assert!(result.is_ok(), "concurrent prompts should not deadlock");

    let (r1, r2) = result.unwrap();
    assert!(r1.is_ok(), "first prompt: {:?}", r1);
    assert!(r2.is_ok(), "second prompt: {:?}", r2);

    let _ = client.close_session(&sid).await;
    let _ = client.shutdown().await;
}

// ===========================================================================
// Process exits with error
// ===========================================================================

#[tokio::test]
async fn process_kill_during_session() {
    let mut child = spawn_agent(&[]).await;
    let transport = {
        let stdin = child.stdin.take().expect("no stdin");
        let stdout = child.stdout.take().expect("no stdout");
        StdioTransport::new(BufReader::new(stdout), stdin)
    };
    let handler: Arc<dyn AcpClientHandler> = Arc::new(NoopClientHandler);
    let mut client = AcpClient::new(transport, handler);

    // Initialize
    let _ = client
        .initialize(
            Some(Implementation {
                name: "nabu-e2e".to_string(),
                title: None,
                version: "0.0.0".to_string(),
                _meta: None,
            }),
            Some(ClientCapabilities::default()),
        )
        .await
        .expect("initialize");

    // Kill the child
    let _ = child.kill().await;

    // Wait for stdout to close
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // new_session should fail
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.new_session("/tmp", vec![], vec![]),
    )
    .await;

    assert!(result.is_ok(), "should get an error, not hang");
    assert!(result.unwrap().is_err(), "new_session should fail after kill");
}
