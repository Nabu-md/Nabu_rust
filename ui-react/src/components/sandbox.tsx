// ──────────────────────────────────────────────────────────────────────────────
// sandbox.tsx — sandboxed iframe for app blocks
//
// Mirrors: ui-react/src/components/sandbox.rs
//
// Renders an <iframe> with srcdoc set to the provided HTML content and the
// `sandbox` attribute set to "allow-scripts allow-forms". Listens for
// postMessage events from the iframe content window and logs them.
//
// Uses a ref + useEffect to attach the message listener after the iframe
// mounts (mirrors the Dioxus Effect::new / NodeRef pattern).
// ──────────────────────────────────────────────────────────────────────────────

import { useEffect, useRef, type ReactNode } from "react";

export interface AppBlockSandboxProps {
  /** Raw HTML content to render inside the sandboxed iframe. */
  htmlContent: string;
}

/**
 * A sandboxed iframe for rendering untrusted app-block HTML.
 *
 * The iframe content window posts messages via `window.postMessage`; this
 * component listens and logs them (matching the Dioxus spec's behaviour).
 */
export function AppBlockSandbox({
  htmlContent,
}: AppBlockSandboxProps): ReactNode {
  const iframeRef = useRef<HTMLIFrameElement>(null);

  useEffect(() => {
    const iframe = iframeRef.current;
    if (!iframe) return;

    const contentWindow = iframe.contentWindow;
    if (!contentWindow) return;

    const handler = (event: MessageEvent) => {
      const data = event.data;
      if (typeof data === "string") {
        // Mirrors leptos::logging::log!("Message from sandbox: {}", data)
        console.log("Message from sandbox:", data);
      }
    };

    contentWindow.addEventListener("message", handler);
    return () => contentWindow.removeEventListener("message", handler);
  }, [htmlContent]);

  return (
    <iframe
      ref={iframeRef}
      srcDoc={htmlContent}
      className="sandbox-frame"
      sandbox="allow-scripts allow-forms"
      title="App Block Sandbox"
    />
  );
}
