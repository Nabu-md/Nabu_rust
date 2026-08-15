use crate::jobs::cancellation::CancellationToken;
use crate::jobs::workers::progress::ProgressReporter;
use crate::models::{CustomPropertyValue, ObjectContent, ObjectType};
use crate::processing::processor::{ProcessingContext, ProcessingResult, Processor, ProcessingStats};
use crate::diagnostic::{Diagnostic, DiagnosticCategory, DiagnosticSeverity, TextPosition, TextRange};
use crate::native::NativeError;
use async_trait::async_trait;
use std::time::Instant;

fn pdf_diagnostic(severity: DiagnosticSeverity, message: String, code: &str) -> Diagnostic {
    Diagnostic::new(
        severity,
        TextRange::empty(TextPosition::new(0, 0)),
        message,
    )
    .with_code(code.to_string())
    .with_source("pdf_text_processor".to_string())
    .with_category(DiagnosticCategory::Ocr)
}

/// Extracts text content from PDF files via the native PDFKit engine
/// ([`crate::native::pdfkit`]). No simulated extraction exists.
///
/// On success the extracted text is:
/// - Stored as `pdf_extracted_text` plain-text custom property.
/// - Stored as a structured `pdf_text_info` JSON custom property (for future UI).
/// - Set as `metadata.description` in full (not truncated to 200 chars) so
///   the Indexer tokenizes every word via `tokenize_str(desc)`.
pub struct PdfTextProcessor;

#[async_trait]
impl Processor for PdfTextProcessor {
    fn name(&self) -> &'static str {
        "pdf_text_processor"
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

        let pdf_data = match &context.object.content {
            ObjectContent::Binary {
                mime_type, data, ..
            } if mime_type == "application/pdf" => data.clone(),
            _ => return ProcessingResult::unmodified(context.object.clone()),
        };

        progress.set_progress(0.4);
        let mut object = context.object.clone();

        let start = Instant::now();
        let engine_result = tokio::task::spawn_blocking(move || crate::native::pdfkit::extract_text(&pdf_data))
            .await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let extracted = match engine_result {
            Ok(Ok(text)) => text,
            Ok(Err(NativeError::UnsupportedPlatform)) => {
                let info = serde_json::json!({
                    "extracted_text": null,
                    "page_count": 0,
                    "extraction_succeeded": false,
                    "warning": "PDFKit unavailable on this platform (requires macOS)",
                });
                object.custom_properties.insert(
                    "pdf_text_info".to_string(),
                    CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diagnostic(
                        DiagnosticSeverity::Information,
                        "PDFKit unavailable on this platform; PDF preserved without text extraction".to_string(),
                        "PDF_PLATFORM_UNSUPPORTED",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("pdf_pages".to_string(), "0".to_string()),
                    );
            }
            Ok(Err(e)) => {
                let err_msg = format!("PDFKit text extraction error: {e}");
                let info = serde_json::json!({
                    "extracted_text": null,
                    "page_count": 0,
                    "extraction_succeeded": false,
                    "warning": err_msg,
                });
                object.custom_properties.insert(
                    "pdf_text_info".to_string(),
                    CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diagnostic(
                        DiagnosticSeverity::Error,
                        format!("PDF text extraction failed: {e}"),
                        "PDF_ENGINE_ERROR",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("pdf_pages".to_string(), "0".to_string()),
                    );
            }
            Err(_) => {
                let info = serde_json::json!({
                    "extracted_text": null,
                    "page_count": 0,
                    "extraction_succeeded": false,
                    "warning": "PDF text extraction task panicked",
                });
                object.custom_properties.insert(
                    "pdf_text_info".to_string(),
                    CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diagnostic(
                        DiagnosticSeverity::Critical,
                        "PDF text extraction task panicked".to_string(),
                        "PDF_PANIC",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("pdf_pages".to_string(), "0".to_string()),
                    );
            }
        };

        progress.set_progress(0.7);

        if !extracted.text.is_empty() {
            object.custom_properties.insert(
                "pdf_extracted_text".to_string(),
                CustomPropertyValue::Text(extracted.text.clone()),
            );

            let info = serde_json::json!({
                "extracted_text": extracted.text,
                "page_count": extracted.page_count,
                "extraction_succeeded": true,
                "warning": serde_json::Value::Null,
            });
            object.custom_properties.insert(
                "pdf_text_info".to_string(),
                CustomPropertyValue::Text(info.to_string()),
            );

            if object.metadata.description.is_none() {
                object.metadata.description = Some(extracted.text.clone());
            }

            object.custom_properties.insert(
                "pdf_text_extracted".to_string(),
                CustomPropertyValue::Text("true".to_string()),
            );

            object.metadata.word_count = Some(extracted.text.split_whitespace().count());
        }

        progress.set_progress(1.0);
        ProcessingResult::new(object)
            .with_stats(
                ProcessingStats::new()
                    .with_duration_ms(duration_ms)
                    .with_metric("pdf_pages".to_string(), extracted.page_count.to_string()),
            )
    }

    fn supports(&self, object_type: &ObjectType) -> bool {
        matches!(object_type, ObjectType::Document | ObjectType::Attachment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::KnowledgeObject;

    fn fixture_pdf() -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/pdf_fixture.pdf"
        );
        std::fs::read(path).expect("pdf fixture should exist")
    }

    #[tokio::test]
    async fn test_pdf_text_extraction_real() {
        let obj = KnowledgeObject::new(
            ObjectType::Document,
            ObjectContent::Binary {
                mime_type: "application/pdf".to_string(),
                data: fixture_pdf(),
                filename: Some("document.pdf".to_string()),
            },
        );

        let ctx = ProcessingContext::new(obj);
        let processor = PdfTextProcessor;
        let result = processor
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        if cfg!(target_os = "macos") {
            // Real PDFKit extraction of a Chrome-printed PDF must contain text.
            let extracted = result
                .object
                .custom_properties
                .get("pdf_extracted_text")
                .map(|v| match v {
                    CustomPropertyValue::Text(s) => s.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            assert!(!extracted.is_empty(), "PDFKit should extract text");
            assert!(extracted.contains("QUICK BROWN FOX"));

            // pdf_info JSON must be stored
            let pdf_info = result
                .object
                .custom_properties
                .get("pdf_text_info")
                .and_then(|v| match v {
                    CustomPropertyValue::Text(s) => serde_json::from_str::<serde_json::Value>(s).ok(),
                    _ => None,
                });
            assert!(pdf_info.is_some(), "pdf_text_info must be stored");
            let parsed = pdf_info.unwrap();
            assert!(parsed["extraction_succeeded"].as_bool().unwrap_or(false), "extraction_succeeded must be true");
            assert!(parsed["page_count"].is_number(), "page_count must be a number");

            // description must contain the FULL text (not truncated to 200 chars)
            let desc = result.object.metadata.description.as_deref().unwrap_or("");
            assert!(desc.contains("QUICK BROWN FOX"), "description must contain the full extracted text");
            assert_eq!(desc, &extracted, "description should be the full extracted text, not truncated");
        } else {
            // Non-macOS: PDFKit unavailable, pdf_text_info must be stored with warning.
            assert!(result.modified, "object should be modified to record PDF platform warning");
            let pdf_info = result
                .object
                .custom_properties
                .get("pdf_text_info")
                .and_then(|v| match v {
                    CustomPropertyValue::Text(s) => serde_json::from_str::<serde_json::Value>(s).ok(),
                    _ => None,
                });
            assert!(pdf_info.is_some(), "pdf_text_info must be stored even on failure");
            let warning = pdf_info.and_then(|v| v["warning"].as_str().map(|s| s.to_string()));
            assert!(warning.is_some(), "warning must be set on non-macOS");
        }
    }

    #[tokio::test]
    async fn test_skips_non_pdf() {
        let obj = KnowledgeObject::new(
            ObjectType::Note,
            ObjectContent::Markdown("Just a note".to_string()),
        );

        let ctx = ProcessingContext::new(obj);
        let processor = PdfTextProcessor;
        let result = processor
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        assert!(!result.modified);
    }
}
