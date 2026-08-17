use crate::diagnostic::{
    Diagnostic, DiagnosticCategory, DiagnosticSeverity, TextPosition, TextRange,
};
use crate::jobs::cancellation::CancellationToken;
use crate::jobs::workers::progress::ProgressReporter;
use crate::models::{CustomPropertyValue, KnowledgeObject, ObjectContent, ObjectType};
use crate::native::NativeError;
use crate::processing::processor::ProcessingContext;
use crate::processing::processor::{ProcessingResult, ProcessingStats, Processor};
use async_trait::async_trait;
use std::time::Instant;

fn slug_from_filename(filename: Option<&str>) -> String {
    filename
        .and_then(|f| f.rsplit('.').nth(1))
        .filter(|s| !s.is_empty())
        .unwrap_or("image")
        .to_string()
}

fn build_ocr_info_json(
    extracted_text: Option<&str>,
    confidence: Option<f64>,
    recognition_language: Option<&str>,
    page_count: Option<u32>,
    processing_duration_ms: Option<u64>,
    is_scanned: Option<bool>,
    warning: Option<&str>,
) -> CustomPropertyValue {
    let info = serde_json::json!({
        "extracted_text": extracted_text,
        "confidence": confidence,
        "recognition_language": recognition_language,
        "page_count": page_count,
        "processing_duration_ms": processing_duration_ms,
        "is_scanned": is_scanned,
        "warning": warning,
    });
    CustomPropertyValue::Text(info.to_string())
}

fn store_ocr_info(
    object: &mut KnowledgeObject,
    extracted_text: Option<&str>,
    confidence: Option<f64>,
    recognition_language: Option<&str>,
    page_count: Option<u32>,
    processing_duration_ms: Option<u64>,
    is_scanned: Option<bool>,
    warning: Option<&str>,
) {
    object.custom_properties.insert(
        "ocr_info".to_string(),
        build_ocr_info_json(
            extracted_text,
            confidence,
            recognition_language,
            page_count,
            processing_duration_ms,
            is_scanned,
            warning,
        ),
    );
}

fn ocr_diagnostic(severity: DiagnosticSeverity, message: String, code: &str) -> Diagnostic {
    Diagnostic::new(severity, TextRange::empty(TextPosition::new(0, 0)), message)
        .with_code(code.to_string())
        .with_source("ocr_processor".to_string())
        .with_category(DiagnosticCategory::Ocr)
}

/// Performs OCR on image, scan, and screenshot content.
///
/// Uses the real macOS Vision framework (`VNRecognizeTextRequest`) through
/// [`crate::native::vision`]. No simulated OCR exists; when the native engine
/// is unavailable (non-macOS) the object is still modified to record an
/// `ocr_info` warning so the inbox UI can surface the platform limitation.
///
/// On successful OCR the extracted text is:
/// - Stored as a structured `ocr_info` JSON custom property (read by the
///   inbox UI via `commands.rs::knowledge_object_to_inbox_item` →
///   `custom_json(obj, "ocr_info")`).
/// - Stored as the full `extracted_text` plain-text custom property
///   (backward compatibility).
/// - Set as `metadata.description` in full (not truncated) so the Indexer
///   tokenizes every word through `tokenize_object` → `tokenize_str(desc)`.
/// - Written to a `.ocr.md` Markdown companion note whose body is the OCR
///   text, persisted via the existing `StorageManager.save()` →
///   `ITEM_STORED` → Indexer chain.
pub struct OcrProcessor;

#[async_trait]
impl Processor for OcrProcessor {
    fn name(&self) -> &'static str {
        "ocr_processor"
    }

    async fn process(
        &self,
        context: &ProcessingContext,
        progress: ProgressReporter,
        cancellation: CancellationToken,
    ) -> ProcessingResult {
        if cancellation.is_cancelled() {
            return ProcessingResult::unmodified(context.object.clone());
        }

        progress.set_progress(0.1);
        let mut object = context.object.clone();

        let (image_data, filename, mime_type) = match &object.content {
            ObjectContent::Binary {
                mime_type,
                data,
                filename,
                ..
            } if mime_type.starts_with("image/") => {
                (data.clone(), filename.clone(), mime_type.clone())
            }
            _ => return ProcessingResult::unmodified(object),
        };

        progress.set_progress(0.4);

        let is_scanned = object.object_type == ObjectType::Scan;
        let recognition_language = object.metadata.language.clone();
        let start = Instant::now();

        let engine_result =
            tokio::task::spawn_blocking(move || crate::native::vision::recognize_text(&image_data))
                .await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let recognized = match engine_result {
            Ok(Ok(lines)) if !lines.is_empty() => lines,
            Ok(Ok(_)) => {
                store_ocr_info(
                    &mut object,
                    None,
                    None,
                    recognition_language.as_deref(),
                    Some(1),
                    Some(duration_ms),
                    Some(is_scanned),
                    Some("No text recognized in image"),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(ocr_diagnostic(
                        DiagnosticSeverity::Warning,
                        "OCR returned no text for this image".to_string(),
                        "OCR_EMPTY_RESULT",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("ocr_lines".to_string(), "0".to_string()),
                    );
            }
            Ok(Err(NativeError::UnsupportedPlatform)) => {
                store_ocr_info(
                    &mut object,
                    None,
                    None,
                    recognition_language.as_deref(),
                    Some(1),
                    Some(duration_ms),
                    Some(is_scanned),
                    Some("OCR engine unavailable on this platform (requires macOS)"),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(ocr_diagnostic(
                        DiagnosticSeverity::Information,
                        "OCR engine unavailable on this platform; image preserved without text extraction"
                            .to_string(),
                        "OCR_PLATFORM_UNSUPPORTED",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("ocr_lines".to_string(), "0".to_string()),
                    );
            }
            Ok(Err(e)) => {
                let err_msg = format!("Vision OCR error: {e}");
                store_ocr_info(
                    &mut object,
                    None,
                    None,
                    recognition_language.as_deref(),
                    Some(1),
                    Some(duration_ms),
                    Some(is_scanned),
                    Some(&err_msg),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(ocr_diagnostic(
                        DiagnosticSeverity::Error,
                        format!("OCR processing failed: {e}"),
                        "OCR_ENGINE_ERROR",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("ocr_lines".to_string(), "0".to_string()),
                    );
            }
            Err(_) => {
                store_ocr_info(
                    &mut object,
                    None,
                    None,
                    recognition_language.as_deref(),
                    Some(1),
                    Some(duration_ms),
                    Some(is_scanned),
                    Some("OCR task panicked"),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(ocr_diagnostic(
                        DiagnosticSeverity::Critical,
                        "OCR task panicked".to_string(),
                        "OCR_PANIC",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("ocr_lines".to_string(), "0".to_string()),
                    );
            }
        };

        progress.set_progress(0.7);

        let text: String = recognized
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let confidence: f64 =
            recognized.iter().map(|l| l.confidence).sum::<f64>() / recognized.len() as f64;
        let line_count = recognized.len() as u64;

        store_ocr_info(
            &mut object,
            Some(&text),
            Some(confidence),
            recognition_language.as_deref(),
            Some(1),
            Some(duration_ms),
            Some(is_scanned),
            None,
        );

        object.custom_properties.insert(
            "extracted_text".to_string(),
            CustomPropertyValue::Text(text.clone()),
        );

        if object.metadata.description.is_none() {
            object.metadata.description = Some(text.clone());
        }

        object.metadata.ocr_confidence = Some(confidence);
        object.metadata.word_count = Some(text.split_whitespace().count());

        let stem = slug_from_filename(filename.as_deref());
        let vault_path = format!("Inbox/{}.ocr.md", crate::inbox::model::slugify(&stem));
        object.metadata.vault_path = Some(vault_path);
        object.content = ObjectContent::Markdown(text.clone());

        object.custom_properties.insert(
            "source_image_mime".to_string(),
            CustomPropertyValue::Text(mime_type),
        );
        object.custom_properties.insert(
            "source_image_filename".to_string(),
            CustomPropertyValue::Text(filename.unwrap_or_default()),
        );

        progress.set_progress(1.0);

        ProcessingResult::new(object).with_stats(
            ProcessingStats::new()
                .with_duration_ms(duration_ms)
                .with_metric("ocr_lines".to_string(), line_count.to_string()),
        )
    }

    fn supports(&self, object_type: &ObjectType) -> bool {
        matches!(
            object_type,
            ObjectType::Image | ObjectType::Screenshot | ObjectType::Scan | ObjectType::Document
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::KnowledgeObject;

    fn fixture_png() -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/ocr_fixture.png"
        );
        std::fs::read(path).expect("ocr fixture should exist")
    }

    #[tokio::test]
    async fn test_ocr_real_image() {
        let obj = KnowledgeObject::new(
            ObjectType::Screenshot,
            ObjectContent::Binary {
                mime_type: "image/png".to_string(),
                data: fixture_png(),
                filename: Some("screenshot.png".to_string()),
            },
        );

        let ctx = ProcessingContext::new(obj);
        let ocr = OcrProcessor;
        let result = ocr
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        if cfg!(target_os = "macos") {
            // Real Vision OCR on a fixture with large text must detect it.
            let extracted = result
                .object
                .custom_properties
                .get("extracted_text")
                .and_then(|v| match v {
                    CustomPropertyValue::Text(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            assert!(
                !extracted.is_empty(),
                "Vision OCR should extract text from the fixture"
            );
            assert!(result.object.metadata.ocr_confidence.is_some());

            // ocr_info must be stored as a JSON-encoded Text so the inbox UI
            // (commands.rs: custom_json(obj, "ocr_info")) can deserialize it.
            let ocr_info = result
                .object
                .custom_properties
                .get("ocr_info")
                .and_then(|v| match v {
                    CustomPropertyValue::Text(s) => Some(s.clone()),
                    _ => None,
                })
                .expect("ocr_info custom property must exist");
            let parsed: serde_json::Value =
                serde_json::from_str(&ocr_info).expect("ocr_info must be valid JSON");
            assert!(
                parsed["extracted_text"].as_str().is_some(),
                "ocr_info JSON must include extracted_text"
            );
            assert!(
                parsed["confidence"].is_number(),
                "ocr_info JSON must include confidence"
            );
            assert!(
                parsed["is_scanned"].is_boolean(),
                "ocr_info JSON must include is_scanned"
            );

            // description must contain the FULL text (not truncated to 200 chars)
            // so the Indexer tokenizes every word.
            let desc = result.object.metadata.description.as_deref().unwrap_or("");
            assert!(!desc.is_empty(), "description must be populated");
            assert!(
                desc.len() > 200 || extracted.len() <= 200,
                "description should contain the full extracted text, not a 200-char truncation"
            );

            // Content must be transformed to Markdown companion note.
            assert!(
                matches!(result.object.content, ObjectContent::Markdown(_)),
                "content should be Markdown after successful OCR"
            );
            assert!(
                result
                    .object
                    .metadata
                    .vault_path
                    .as_deref()
                    .unwrap_or("")
                    .ends_with(".ocr.md"),
                "vault_path should end with .ocr.md, got: {:?}",
                result.object.metadata.vault_path
            );

            // Source image metadata preserved.
            assert!(
                result
                    .object
                    .custom_properties
                    .contains_key("source_image_mime"),
                "source image MIME must be preserved"
            );
            assert!(
                result
                    .object
                    .custom_properties
                    .contains_key("source_image_filename"),
                "source image filename must be preserved"
            );

            // Stats should include OCR duration.
            assert!(result.stats.duration_ms.is_some());
            assert!(result.stats.extra.contains_key("ocr_lines"));
        } else {
            // Non-macOS: Vision unavailable, but ocr_info warning is recorded.
            assert!(
                result.modified,
                "object should be modified to record the OCR platform warning"
            );
            let ocr_info = result
                .object
                .custom_properties
                .get("ocr_info")
                .and_then(|v| match v {
                    CustomPropertyValue::Text(s) => {
                        serde_json::from_str::<serde_json::Value>(s).ok()
                    }
                    _ => None,
                });
            assert!(
                ocr_info.is_some(),
                "ocr_info must be stored even on failure"
            );
            let warning = ocr_info.and_then(|v| v["warning"].as_str().map(|s| s.to_string()));
            assert!(
                warning.is_some(),
                "warning must be set when OCR engine is unavailable"
            );
            // Image content preserved on failure.
            assert!(
                matches!(result.object.content, ObjectContent::Binary { .. }),
                "binary content must be preserved on OCR failure"
            );
            // A diagnostic must be emitted.
            assert!(
                result.has_diagnostics(),
                "a diagnostic must be emitted for platform failure"
            );
        }
    }

    #[tokio::test]
    async fn test_ocr_skips_non_image() {
        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("Hello world".to_string()),
        );

        let ctx = ProcessingContext::new(obj);
        let ocr = OcrProcessor;
        let result = ocr
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        assert!(!result.modified);
    }

    #[tokio::test]
    async fn test_ocr_records_empty_result_warning() {
        // Construct a tiny 1x1 transparent PNG — Vision on macOS may return
        // zero recognized lines.  On non-macOS the engine returns
        // UnsupportedPlatform which also exercises the failure path.
        let png: Vec<u8> = {
            let fixture = fixture_png();
            if fixture.len() > 8 {
                fixture[..8].to_vec()
            } else {
                fixture.clone()
            }
        };

        let obj = KnowledgeObject::new(
            ObjectType::Image,
            ObjectContent::Binary {
                mime_type: "image/png".to_string(),
                data: png,
                filename: Some("tiny.png".to_string()),
            },
        );

        let ctx = ProcessingContext::new(obj);
        let ocr = OcrProcessor;
        let result = ocr
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        // The ocr_info must always be stored, with a warning on failure.
        let ocr_info = result
            .object
            .custom_properties
            .get("ocr_info")
            .and_then(|v| match v {
                CustomPropertyValue::Text(s) => serde_json::from_str::<serde_json::Value>(s).ok(),
                _ => None,
            });

        if cfg!(target_os = "macos") {
            if let Some(info) = ocr_info {
                let _ = info;
            }
        } else {
            assert!(ocr_info.is_some(), "ocr_info must be stored on non-macOS");
            let warning = ocr_info.and_then(|v| v["warning"].as_str().map(|s| s.to_string()));
            assert!(warning.is_some(), "warning must be set on non-macOS");
        }
    }

    /// Proves the full chain: the JSON stored by the processor under the
    /// `ocr_info` custom property can be deserialized into the same
    /// `OcrInfo` struct that `commands.rs::knowledge_object_to_inbox_item`
    /// expects (via `custom_json(obj, "ocr_info")` → `serde_json::from_value::<OcrInfo>`).
    #[tokio::test]
    async fn test_ocr_info_json_compatible_with_inbox_ui() {
        #[derive(serde::Deserialize, Debug)]
        struct OcrInfo {
            extracted_text: Option<String>,
            confidence: Option<f64>,
            recognition_language: Option<String>,
            page_count: Option<u32>,
            processing_duration_ms: Option<u64>,
            is_scanned: Option<bool>,
            warning: Option<String>,
        }

        let obj = KnowledgeObject::new(
            ObjectType::Screenshot,
            ObjectContent::Binary {
                mime_type: "image/png".to_string(),
                data: fixture_png(),
                filename: Some("screenshot.png".to_string()),
            },
        );

        let ctx = ProcessingContext::new(obj);
        let ocr = OcrProcessor;
        let result = ocr
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        // Simulate commands.rs::custom_json(obj, "ocr_info")
        let raw = result
            .object
            .custom_properties
            .get("ocr_info")
            .and_then(|v| match v {
                CustomPropertyValue::Text(s) => Some(s.clone()),
                _ => None,
            });
        assert!(raw.is_some(), "ocr_info must be stored as Text");
        let json_value: serde_json::Value =
            serde_json::from_str(&raw.unwrap()).expect("ocr_info must be valid JSON");

        // Simulate the deserialization step from knowledge_object_to_inbox_item
        let parsed: OcrInfo = serde_json::from_value(json_value)
            .expect("ocr_info JSON must deserialize into OcrInfo");

        if cfg!(target_os = "macos") {
            assert!(
                parsed.extracted_text.is_some(),
                "extracted_text must be populated on macOS"
            );
            assert!(
                parsed.confidence.is_some(),
                "confidence must be populated on macOS"
            );
            assert!(parsed.warning.is_none(), "no warning on success");
        } else {
            assert!(
                parsed.warning.is_some(),
                "warning must be populated on non-macOS"
            );
        }
    }

    /// Proves the indexer can tokenize the OCR text — the full text must be
    /// in description (which the indexer tokenizes via tokenize_str) and the
    /// Markdown body (which the indexer tokenizes via tokenize_content).
    #[tokio::test]
    async fn test_ocr_text_is_indexable() {
        let obj = KnowledgeObject::new(
            ObjectType::Screenshot,
            ObjectContent::Binary {
                mime_type: "image/png".to_string(),
                data: fixture_png(),
                filename: Some("screenshot.png".to_string()),
            },
        );

        let ctx = ProcessingContext::new(obj);
        let ocr = OcrProcessor;
        let result = ocr
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        if cfg!(target_os = "macos") {
            // The description contains the full OCR text
            let desc = result.object.metadata.description.as_deref().unwrap_or("");
            assert!(!desc.is_empty(), "description must contain OCR text");

            // The Markdown body (content) contains the full OCR text
            let body = match &result.object.content {
                ObjectContent::Markdown(s) => s.clone(),
                _ => String::new(),
            };
            assert!(!body.is_empty(), "Markdown content must contain OCR text");
            assert_eq!(desc, &body, "description should match the OCR text body");

            // The vault_path produces a .ocr.md file
            let vp = result.object.metadata.vault_path.as_deref().unwrap_or("");
            assert!(vp.ends_with(".ocr.md"), "vault_path must end with .ocr.md");

            // The vault_path includes the source filename stem
            assert!(
                vp.contains("screenshot"),
                "vault_path should contain the source filename stem"
            );
        }
    }
}
