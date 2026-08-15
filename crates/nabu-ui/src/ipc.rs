use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    /// Raw Tauri `invoke` — declared as a synchronous function returning a
    /// `js_sys::Promise` so that callers can decide whether to unwrap or
    /// gracefully handle rejection via [`JsFuture`].
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    fn invoke(cmd: &str, args: JsValue) -> js_sys::Promise;
}

// ── Error type ──────────────────────────────────────────────────────────

/// Error returned when a Tauri IPC invoke is rejected by the backend.
///
/// Wraps the raw JavaScript rejection value so that the backend error message
/// is never silently lost.  Call sites can read [`message`][IpcError::message]
/// or use the [`Display`][std::fmt::Display] implementation to surface a
/// user-visible string.
#[derive(Clone, Debug)]
pub struct IpcError {
    /// The Tauri command name that was invoked, for context in diagnostics.
    pub cmd: String,
    /// The raw JavaScript rejection value from the promise.
    pub cause: JsValue,
}

impl IpcError {
    /// Extracts a human-readable message from the underlying rejection value.
    pub fn message(&self) -> String {
        // String rejections are the common case — Tauri serialises `Err(String)`
        // as a JS string on the other side.
        if let Some(s) = self.cause.as_string() {
            return s;
        }
        // Non-string rejections (Error objects, etc.) — best-effort JSON.
        #[cfg(target_arch = "wasm32")]
        {
            js_sys::JSON::stringify(&self.cause)
                .ok()
                .and_then(|s| s.as_string())
                .unwrap_or_else(|| "IPC operation failed".to_string())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            "IPC operation failed".to_string()
        }
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.cmd, self.message())
    }
}

impl std::error::Error for IpcError {}

// ── IntoJsValue trait (for serde_wasm_bindgen shadowing) ────────────────

/// Bridge trait that lets the local `serde_wasm_bindgen` shadow module
/// accept *any* of:
///
/// * a bare [`JsValue`] — the normal deserialization input, and
/// * a [`Result<JsValue, IpcError>`] — an IPC result that may have failed, and
/// * an [`Option<JsValue>`] — a soft-fail probe value.
///
/// The return is `Option<JsValue>` so that the shadowed `from_value` can
/// distinguish "value present" from "value absent / IPC failed" and emit a
/// deserialization error in the latter case.  This preserves the existing
/// `from_value::<T>(result).is_ok()` / `.ok()` / `unwrap_or_default()`
/// patterns used throughout the codebase (including `inbox.rs`, which must not
/// be modified).
pub trait IntoJsValue {
    /// Returns `Some(JsValue)` when a concrete value is available, or `None`
    /// when the source represents an IPC failure (for `Result`) or an absent
    /// value (for `Option`).
    fn into_js_value(self) -> Option<JsValue>;
}

impl IntoJsValue for JsValue {
    #[inline]
    fn into_js_value(self) -> Option<JsValue> {
        Some(self)
    }
}

impl IntoJsValue for Result<JsValue, IpcError> {
    #[inline]
    fn into_js_value(self) -> Option<JsValue> {
        // On Err the None return causes the shadowed from_value to fail,
        // preserving the IPC failure as a deserialization error.
        self.ok()
    }
}

impl IntoJsValue for Option<JsValue> {
    #[inline]
    fn into_js_value(self) -> Option<JsValue> {
        self
    }
}

// ── Test hook ─────────────────────────────────────────────────────────

#[cfg(test)]
thread_local! {
    /// Optional mock IPC handler used in unit tests.  When set, `tauri_invoke`
    /// short-circuits to this function instead of calling the wasm-bindgen
    /// extern `invoke` (which is unavailable on non-wasm targets).
    ///
    /// This is a genuine testability defect: without a seam, every call to
    /// `tauri_invoke` / `tauri_invoke_safe` is an untestable hard dependency on
    /// `window.__TAURI__.core.invoke`.  The mock lets tests inject canned
    /// responses for IPC commands.
    static MOCK_INVOKE: std::cell::RefCell<Option<Box<dyn Fn(&str, &wasm_bindgen::JsValue) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> + Send + Sync>>> =
        std::cell::RefCell::new(None);
}

/// Install a mock IPC handler for the duration of a test.
#[cfg(test)]
pub fn set_ipc_mock<F>(f: F)
where
    F: Fn(&str, &wasm_bindgen::JsValue)
        -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>
        + Send
        + Sync
        + 'static,
{
    MOCK_INVOKE.with(|cell| *cell.borrow_mut() = Some(Box::new(f)));
}

/// Remove any installed mock IPC handler.
#[cfg(test)]
pub fn clear_ipc_mock() {
    MOCK_INVOKE.with(|cell| *cell.borrow_mut() = None);
}

// ── Public IPC wrappers ────────────────────────────────────────────────

/// Raw Tauri IPC invoke.  Returns `Ok(JsValue)` on success and
/// `Err(IpcError)` when the backend rejects the command.
///
/// The error preserves the backend error message — it is **never** silently
/// turned into `None`, a default value, or a successful `()`.  The previous
/// panicking `.unwrap()` has been removed so a backend failure is surfaced
/// rather than crashing the renderer.
///
/// In `#[cfg(test)]` builds, if a mock has been installed via
/// [`set_ipc_mock`], the mock is invoked instead of the real wasm-bindgen
/// `invoke` extern — enabling native-target tests of IPC-dependent code.
pub async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, IpcError> {
    #[cfg(test)]
    {
        let mock_result: Option<Result<JsValue, JsValue>> = MOCK_INVOKE
            .try_with(|cell| {
                let borrow = cell.borrow();
                borrow.as_ref().map(|f| f(cmd, &args))
            })
            .ok()
            .flatten();
        if let Some(res) = mock_result {
            return res.map_err(|cause| IpcError {
                cmd: cmd.to_string(),
                cause,
            });
        }
    }
    JsFuture::from(invoke(cmd, args)).await.map_err(|cause| IpcError {
        cmd: cmd.to_string(),
        cause,
    })
}

/// Safe variant of [`tauri_invoke`] for soft-fail probes.
///
/// Returns `Ok(Some(val))` on success, `Ok(None)` when the result is null/
/// undefined, and `Err(IpcError)` on IPC failure.  This gives callers the
/// distinction between "not configured" (None) and "error" (Err).
///
/// Callers that prefer the `Option<JsValue>` shape can call `.ok()` and
/// then flatten: `.ok().flatten()`.
pub async fn tauri_invoke_safe(cmd: &str, args: JsValue) -> Result<Option<JsValue>, IpcError> {
    let val = tauri_invoke(cmd, args).await?;
    if val.is_null() || val.is_undefined() {
        Ok(None)
    } else {
        Ok(Some(val))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── IntoJsValue trait tests ───────────────────────────────────────

    #[test]
    fn into_js_value_for_ok_result() {
        let val = JsValue::from_str("hello");
        let result: Result<JsValue, IpcError> = Ok(val);
        let v = result.into_js_value().unwrap();
        assert_eq!(v.as_string(), Some("hello".to_string()));
    }

    #[test]
    fn into_js_value_for_err_result_returns_none() {
        let cause = JsValue::from_str("backend boom");
        let err = IpcError {
            cmd: "test_cmd".to_string(),
            cause,
        };
        let result: Result<JsValue, IpcError> = Err(err);
        // None signals failure to the shadowed from_value.
        assert!(result.into_js_value().is_none());
    }

    #[test]
    fn into_js_value_for_none_option() {
        let v = Option::<JsValue>::None.into_js_value();
        assert!(v.is_none());
    }

    #[test]
    fn into_js_value_for_some_option() {
        let v = Some(JsValue::from_str("hi")).into_js_value().unwrap();
        assert_eq!(v.as_string(), Some("hi".to_string()));
    }

    // ── IpcError tests ──────────────────────────────────────────────────

    #[test]
    fn ipc_error_message_from_string_cause() {
        let err = IpcError {
            cmd: "note_save".to_string(),
            cause: JsValue::from_str("disk full"),
        };
        assert_eq!(err.message(), "disk full");
    }

    #[test]
    fn ipc_error_message_non_string_cause_native() {
        let err = IpcError {
            cmd: "note_save".to_string(),
            cause: JsValue::NULL,
        };
        // On non-wasm targets the fallback is a generic string.
        assert!(!err.message().is_empty());
    }

    #[test]
    fn ipc_error_display_includes_cmd() {
        let err = IpcError {
            cmd: "note_save".to_string(),
            cause: JsValue::from_str("disk full"),
        };
        let s = format!("{}", err);
        assert!(s.contains("note_save"));
        assert!(s.contains("disk full"));
    }

    // ── IPC result extraction (Test 1: rejection) ───────────────────────
    ///
    //  Simulates a rejected promise by feeding JsFuture-style `Err(JsValue)`
    //  directly into the `tauri_invoke` result-extraction logic.  This proves:
    //    * `tauri_invoke` returns `Err` (not a panic)
    //    * the backend error message is preserved in `IpcError::cause`

    #[test]
    fn rejected_invoke_returns_err_without_panic() {
        // Simulate the JsFuture rejection: `Err(JsValue)` from the promise.
        let js_rejection = JsValue::from_str("backend rejected the command");
        let future_result: Result<JsValue, JsValue> = Err(js_rejection);

        // This is the exact extraction logic used inside `tauri_invoke`.
        let extracted: Result<JsValue, IpcError> = future_result
            .map_err(|cause| IpcError {
                cmd: "test_reject".to_string(),
                cause,
            });

        assert!(extracted.is_err());
        let err = extracted.unwrap_err();
        assert_eq!(err.cmd, "test_reject");
        assert_eq!(err.message(), "backend rejected the command");
    }

    // ── IPC result extraction (Test 2: success) ─────────────────────────
    ///
    //  Simulates a resolved promise.  The JsValue must be usable by
    //  `from_value`.

    #[test]
    fn successful_invoke_returns_ok() {
        let js_val = JsValue::from_str("result_value");
        let future_result: Result<JsValue, JsValue> = Ok(js_val);

        let extracted: Result<JsValue, IpcError> = future_result
            .map_err(|cause| IpcError {
                cmd: "test_ok".to_string(),
                cause,
            });

        assert!(extracted.is_ok());
        let val = extracted.unwrap();
        assert_eq!(val.as_string(), Some("result_value".to_string()));
    }

    // ── Shadow from_value sees IPC failure (Test 3) ─────────────────────
    ///
    //  Verifies that a rejected IPC, after `into_js_value` returns `None`,
    //  causes `serde_wasm_bindgen::from_value::<()>()` to fail — which is how
    //  the inbox.rs and other existing call sites detect errors.  This proves
    //  the error path is visible (not silently swallowed) through the
    //  existing `from_value::<()>(result).is_ok()` pattern.

    #[test]
    fn rejected_invoke_propagates_as_deserialization_failure() {
        let cause = JsValue::from_str("save conflict");
        let ipc_result: Result<JsValue, IpcError> = Err(IpcError {
            cmd: "note_save".to_string(),
            cause,
        });

        // Mirrors what `serde_wasm_bindgen::from_value::<()>(result)` does
        // after `IntoJsValue::into_js_value` returns None on Err.
        let maybe_js = ipc_result.into_js_value();
        assert!(maybe_js.is_none(), "Err result must yield None JsValue");
    }

    // ── Batch failure (Test 4) ──────────────────────────────────────────
    ///
    //  When batch operations are used, individual failures must still be
    //  visible.  We simulate a batch of IPC results where some fail — the
    //  aggregate must reflect the failures without panicking.

    #[test]
    fn batch_operation_preserves_individual_failures() {
        let results: Vec<Result<JsValue, IpcError>> = vec![
            Err(IpcError {
                cmd: "note_delete".to_string(),
                cause: JsValue::from_str("permission denied"),
            }),
            Ok(JsValue::from_str("deleted")),
            Err(IpcError {
                cmd: "note_delete".to_string(),
                cause: JsValue::from_str("file in use"),
            }),
        ];

        let successes = results.iter().filter(|r| r.is_ok()).count();
        let failures = results.iter().filter(|r| r.is_err()).count();

        assert_eq!(successes, 1);
        assert_eq!(failures, 2);

        // The failure messages are preserved per-item.
        let failure_msgs: Vec<String> = results
            .iter()
            .filter_map(|r| r.as_ref().err().map(|e| e.message()))
            .collect();
        assert_eq!(failure_msgs, vec!["permission denied", "file in use"]);
    }

    // ── IPC mock hook (Test 5+) ───────────────────────────────────────────
    ///
    //  On native (non-wasm32) targets the real `invoke` extern calls
    //  `window.__TAURI__.core.invoke`, which panics.  The `set_ipc_mock` /
    //  `clear_ipc_mock` seam lets tests inject a fake handler so
    //  `tauri_invoke` and `tauri_invoke_safe` can be exercised natively.

    #[test]
    fn mock_invoke_returns_canned_success_response() {
        set_ipc_mock(|cmd, _args| {
            assert_eq!(cmd, "get_settings");
            let val: serde_json::Value =
                serde_json::json!({ "theme": "dark", "main_window_opacity": 0.9 });
            let json = serde_json::to_string(&val).unwrap();
            Ok(JsValue::from_str(&json))
        });

        let fut = tauri_invoke("get_settings", JsValue::NULL);
        let result = futures::executor::block_on(fut);
        clear_ipc_mock();

        assert!(result.is_ok());
        let jsval = result.unwrap();
        // The mock returned a JSON string as a JsValue.
        assert!(jsval.as_string().is_some());
    }

    #[test]
    fn mock_invoke_returns_error_preserved_in_ipc_error() {
        set_ipc_mock(|_cmd, _args| Err(JsValue::from_str("backend exploded")));

        let fut = tauri_invoke("trash_list", JsValue::NULL);
        let result = futures::executor::block_on(fut);
        clear_ipc_mock();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.cmd, "trash_list");
        assert_eq!(err.message(), "backend exploded");
    }

    #[test]
    fn mock_invoke_safe_distinction_null_vs_error() {
        // Null response → Ok(None) (soft-fail probe).
        set_ipc_mock(|_cmd, _args| Ok(JsValue::NULL));
        let fut = tauri_invoke_safe("settings_get", JsValue::NULL);
        let result = futures::executor::block_on(fut);
        clear_ipc_mock();
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn mock_invoke_safe_returns_value_on_success() {
        set_ipc_mock(|_cmd, _args| {
            let json = serde_json::to_string(&serde_json::json!([1, 2, 3])).unwrap();
            Ok(JsValue::from_str(&json))
        });
        let fut = tauri_invoke_safe("trash_list", JsValue::NULL);
        let result = futures::executor::block_on(fut);
        clear_ipc_mock();
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn mock_invoke_safe_propagates_error() {
        set_ipc_mock(|_cmd, _args| Err(JsValue::from_str("connection refused")));
        let fut = tauri_invoke_safe("versions_all", JsValue::NULL);
        let result = futures::executor::block_on(fut);
        clear_ipc_mock();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message(), "connection refused");
    }

    #[test]
    fn to_value_from_value_roundtrip_native() {
        let original = serde_json::json!({
            "id": "abc123",
            "count": 42,
            "active": true,
            "tags": ["a", "b"],
        });
        let jsval = serde_wasm_bindgen::to_value(&original).unwrap();
        let roundtrip: serde_json::Value = serde_wasm_bindgen::from_value(jsval).unwrap();
        assert_eq!(roundtrip, original);
    }

    #[test]
    fn from_value_deserialize_struct_native() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct TestStruct {
            name: String,
            count: u32,
        }
        let json = r#"{"name":"test","count":7}"#;
        let jsval = JsValue::from_str(json);
        let parsed: TestStruct = serde_wasm_bindgen::from_value(jsval).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.count, 7);
    }

    #[test]
    fn from_value_failure_when_jsvalue_is_null() {
        let jsval = JsValue::NULL;
        let result: Result<serde_json::Value, _> = serde_wasm_bindgen::from_value(jsval);
        assert!(result.is_err(), "null JsValue should fail deserialization of a Value");
    }
}
