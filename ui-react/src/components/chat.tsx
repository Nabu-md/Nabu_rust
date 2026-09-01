// ─────────────────────────────────────────────────────────────────────────────
// components/chat — ACP agent chat surface
//
// Mirrors: ui-react/src/components/chat.rs
//
// Renders a live streaming conversation from an ACP agent session.
// Uses StreamingProvider for token rendering and bridges user input
// through Tauri IPC commands (acp_connect, acp_send_message,
// acp_cancel, acp_disconnect).
// ─────────────────────────────────────────────────────────────────────────────

import { createContext, useContext, useEffect, useState } from "react";
import { StreamingProvider, StreamingContainer, StreamingContent, useStreaming } from "../streaming";
import { useToast, type ToastContextValue } from "../context";
import { acpConnect, acpSendMessage, acpCancel, acpDisconnect, acpPermissionRespond } from "../ipc";
import type { AcpPermissionRequestEvent, PermissionOutcome } from "../types";

// ── Chat State ──────────────────────────────────────────────────────────────

export type ChatState = "disconnected" | "connecting" | "connected";

interface ChatContextValue {
  state: ChatState;
  setState: (s: ChatState) => void;
  threadId: string | null;
  setThreadId: (id: string | null) => void;
}

const ChatContext = createContext<ChatContextValue | null>(null);

export function useChat(): ChatContextValue {
  const ctx = useContext(ChatContext);
  if (!ctx) throw new Error("useChat must be used within ChatView");
  return ctx;
}

// ── Props ───────────────────────────────────────────────────────────────────

export interface ChatViewProps {
  className?: string;
}

// ── ChatView ────────────────────────────────────────────────────────────────

export function ChatView({ className = "" }: ChatViewProps) {
  const toasts = useToast();
  const [state, setState] = useState<ChatState>("disconnected");
  const [threadId, setThreadId] = useState<string | null>(null);
  const [agentCommand, setAgentCommand] = useState("codex");
  const [pendingPermission, setPendingPermission] = useState<AcpPermissionRequestEvent | null>(null);

  // Listen for permission requests from the ACP agent via "nabu-event" channel.
  useEffect(() => {
    const g = window as unknown as {
      __TAURI__?: {
        event?: {
          listen: (
            event: string,
            handler: (event: { payload: unknown }) => void
          ) => Promise<() => void>;
        };
      };
    };
    if (typeof g !== "undefined" && g.__TAURI__?.event?.listen) {
      g.__TAURI__.event.listen("nabu-event", (event: { payload: unknown }) => {
        const payload = event.payload as { event_type?: string; payload?: Record<string, unknown> };
        if (payload?.event_type === "AcpPermissionRequested") {
          const req = payload.payload as Record<string, unknown>;
          setPendingPermission(req as unknown as AcpPermissionRequestEvent);
          const title = typeof req.tool_call_title === "string" ? req.tool_call_title : String(req.tool_call_id ?? "");
          toasts.toast(`Agent wants permission: ${title}`, { variant: "warning" });
        }
      });
    }
  }, [toasts]);

  const chatCtx: ChatContextValue = { state, setState, threadId, setThreadId };

  return (
    <ChatContext.Provider value={chatCtx}>
      <div className={["flex flex-col h-full", className].filter(Boolean).join(" ")}>
        <div className="flex-1 overflow-y-auto">
          <StreamingProvider>
            <StreamingContainer>
              <StreamingContent className="pb-4" />
            </StreamingContainer>
          </StreamingProvider>
        </div>

        {/* Permission request banner */}
        {pendingPermission && (
          <PermissionBanner
            request={pendingPermission}
            onRespond={(outcome) => {
              respondToPermission(threadId, pendingPermission.request_id, outcome, toasts);
              setPendingPermission(null);
            }}
          />
        )}

        {/* Connection prompt (disconnected) */}
        {state === "disconnected" && (
          <div className="p-4 border-t border-gray-800 bg-gray-900/50">
            <div className="max-w-3xl mx-auto space-y-3">
              <input
                type="text"
                placeholder='Agent command (e.g. "codex")'
                value={agentCommand}
                onChange={(e) => setAgentCommand(e.target.value)}
                className="w-full px-3 py-2 text-sm border border-gray-700 rounded-lg bg-gray-800 focus:outline-none focus:ring-2 focus:ring-blue-500"
              />
              <button
                type="button"
                onClick={() => connectToAgent(agentCommand, setState, setThreadId, toasts)}
                className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-lg hover:bg-blue-700"
              >
                Connect to Agent
              </button>
            </div>
          </div>
        )}

        {/* Chat input (connected) */}
        {state === "connected" && <ChatInputArea />}
      </div>
    </ChatContext.Provider>
  );
}

// ── Permission Banner ───────────────────────────────────────────────────────

interface PermissionBannerProps {
  request: AcpPermissionRequestEvent;
  onRespond: (outcome: PermissionOutcome) => void;
}

function PermissionBanner({ request, onRespond }: PermissionBannerProps) {
  const title = request.tool_call_title ?? request.tool_call_id;
  return (
    <div className="border-t border-gray-800 bg-gray-900/50 p-4">
      <div className="max-w-3xl mx-auto">
        <div className="flex items-start gap-3">
          <span className="text-blue-400">ⓘ</span>
          <div className="flex-1">
            <div className="text-sm font-medium text-gray-100">Agent requests permission</div>
            <div className="text-xs text-gray-500">{title}</div>
          </div>
        </div>
        <div className="flex gap-2 mt-2">
          {request.options.map((opt) => (
            <button
              key={opt.option_id}
              type="button"
              className="px-3 py-1 text-xs font-medium text-white bg-blue-700 rounded hover:bg-blue-600"
              onClick={() => onRespond({ selected: { option_id: opt.option_id } })}
            >
              {opt.name}
            </button>
          ))}
          <button
            type="button"
            className="px-3 py-1 text-xs font-medium text-gray-300 hover:text-gray-200"
            onClick={() => onRespond({ cancelled: true })}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Chat Input Area ─────────────────────────────────────────────────────────

function ChatInputArea() {
  const ctx = useChat();
  const streaming = useStreaming();
  const toasts = useToast();
  const isStreaming = streaming.has_active;
  const [inputValue, setInputValue] = useState("");

  const btnClass = isStreaming
    ? "px-4 py-2 text-sm font-medium text-white bg-red-600 rounded-lg hover:bg-red-700"
    : "px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-lg hover:bg-blue-700";

  const sendMessage = async () => {
    const msg = inputValue.trim();
    if (!msg || !ctx.threadId) return;
    setInputValue("");
    try {
      await acpSendMessage(ctx.threadId, msg);
    } catch {
      toasts.toast("Message failed", { variant: "error" });
    }
  };

  const cancelStream = async () => {
    if (!ctx.threadId) return;
    try {
      await acpCancel(ctx.threadId);
      toasts.toast("Cancelled", { variant: "info" });
    } catch {
      toasts.toast("Cancel failed", { variant: "error" });
    }
  };

  const disconnect = async () => {
    if (!ctx.threadId) return;
    try {
      await acpDisconnect(ctx.threadId);
      ctx.setState("disconnected");
      ctx.setThreadId(null);
      toasts.toast("Disconnected", { variant: "info" });
    } catch {
      toasts.toast("Disconnect failed", { variant: "error" });
    }
  };

  return (
    <div className="p-4 border-t border-gray-800 bg-gray-900/50">
      <div className="max-w-3xl mx-auto flex gap-3">
        <textarea
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void sendMessage();
            }
          }}
          placeholder="Type your message…"
          rows={3}
          className="flex-1 px-3 py-2 text-sm border border-gray-700 rounded-lg bg-gray-800 focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
        />
        {isStreaming ? (
          <button type="button" className={btnClass} onClick={cancelStream}>Cancel</button>
        ) : (
          <button type="button" className={btnClass} onClick={sendMessage}>Send</button>
        )}
        <button
          type="button"
          className="px-4 py-2 text-sm font-medium text-gray-300 bg-gray-800 border border-gray-700 rounded-lg hover:bg-gray-700"
          onClick={disconnect}
        >
          Disconnect
        </button>
      </div>
    </div>
  );
}

// ── IPC helpers ─────────────────────────────────────────────────────────────

async function connectToAgent(
  cmd: string,
  setState: (s: ChatState) => void,
  setThreadId: (id: string | null) => void,
  toasts: ToastContextValue,
) {
  const parts = cmd.split(/\s+/).filter(Boolean);
  if (parts.length === 0) {
    toasts.toast("Invalid command", { variant: "error" });
    return;
  }
  const command = parts[0];
  const args = parts.slice(1);

  setState("connecting");
  try {
    const result = await acpConnect({ agent_command: [command, ...args] });
    setThreadId(result.thread_id);
    setState("connected");
    toasts.toast(`Connected to ACP agent (session: ${result.session_id})`, { variant: "success" });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    toasts.toast(`Connection failed: ${msg}`, { variant: "error" });
    setState("disconnected");
  }
}

async function respondToPermission(
  threadId: string | null,
  requestId: string,
  outcome: PermissionOutcome,
  toasts: ToastContextValue,
) {
  if (!threadId) {
    toasts.toast("Not connected", { variant: "error" });
    return;
  }
  const outcomeStr = "selected" in outcome ? "Approved" : "Denied";
  try {
    await acpPermissionRespond(threadId, requestId, outcomeStr);
    toasts.toast("Permission sent", { variant: "success" });
  } catch {
    toasts.toast("Permission failed", { variant: "error" });
  }
}
