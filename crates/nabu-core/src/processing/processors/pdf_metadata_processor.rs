use crate::diagnostic::{
    Diagnostic, DiagnosticCategory, DiagnosticSeverity, TextPosition, TextRange,
};
use crate::jobs::cancellation::CancellationToken;
use crate::jobs::workers::progress::ProgressReporter;
use crate::models::{CustomPropertyValue, ObjectContent, ObjectType};
use crate::native::NativeError;
use crate::processing::processor::{
    ProcessingContext, ProcessingResult, ProcessingStats, Processor,
};
use async_trait::async_trait;
use std::time::Instant;

fn pdf_diag(severity: DiagnosticSeverity, message: String, code: &str) -> Diagnostic {
    Diagnostic::new(severity, TextRange::empty(TextPosition::new(0, 0)), message)
        .with_code(code.to_string())
        .with_source("pdf_metadata_processor".to_string())
        .with_category(DiagnosticCategory::Metadata)
}

/// Extracts metadata from PDF files (title, author, pages, etc.) via the
/// native PDFKit engine ([`crate::native::pdfkit`]). No simulation.
///
/// On success the metadata is written into structured custom properties
/// (`pdf_page_count`, `pdf_metadata_extracted`) and a structured `pdf_info`
/// JSON blob for future UI consumption.  On failure (including
/// non-macOS where PDFKit is unavailable) a `pdf_info` JSON with a warning
/// is still stored and a diagnostic is emitted so the object is never
/// silently dropped.
pub struct PdfMetadataProcessor;

#[async_trait]
impl Processor for PdfMetadataProcessor {
    fn name(&self) -> &'static str {
        "pdf_metadata_processor"
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

        progress.set_progress(0.3);
        let mut object = context.object.clone();
        let start = Instant::now();

        let engine_result =
            tokio::task::spawn_blocking(move || crate::native::pdfkit::extract_metadata(&pdf_data))
                .await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let metadata = match engine_result {
            Ok(Ok(meta)) => meta,
            Ok(Err(NativeError::UnsupportedPlatform)) => {
                let info = serde_json::json!({
                    "page_count": 0,
                    "title": serde_json::Value::Null,
                    "author": serde_json::Value::Null,
                    "subject": serde_json::Value::Null,
                    "creator": serde_json::Value::Null,
                    "metadata_extracted": false,
                    "warning": "PDFKit unavailable on this platform (requires macOS)",
                });
                object.custom_properties.insert(
                    "pdf_info".to_string(),
                    CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diag(
                        DiagnosticSeverity::Information,
                        "PDFKit unavailable on this platform; PDF preserved without metadata extraction"
                            .to_string(),
                        "PDF_PLATFORM_UNSUPPORTED",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("pdf_pages".to_string(), "0".to_string()),
                    );
            }
            Ok(Err(e)) => {
                let err_msg = format!("PDFKit metadata extraction error: {e}");
                let info = serde_json::json!({
                    "page_count": 0,
                    "title": serde_json::Value::Null,
                    "author": serde_json::Value::Null,
                    "subject": serde_json::Value::Null,
                    "creator": serde_json::Value::Null,
                    "metadata_extracted": false,
                    "warning": err_msg,
                });
                object.custom_properties.insert(
                    "pdf_info".to_string(),
                    CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diag(
                        DiagnosticSeverity::Error,
                        format!("PDF metadata extraction failed: {e}"),
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
                    "page_count": 0,
                    "title": serde_json::Value::Null,
                    "author": serde_json::Value::Null,
                    "subject": serde_json::Value::Null,
                    "creator": serde_json::Value::Null,
                    "metadata_extracted": false,
                    "warning": "PDF metadata extraction task panicked",
                });
                object.custom_properties.insert(
                    "pdf_info".to_string(),
                    CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diag(
                        DiagnosticSeverity::Critical,
                        "PDF metadata extraction task panicked".to_string(),
                        "PDF_PANIC",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("pdf_pages".to_string(), "0".to_string()),
                    );
            }
        };

        progress.set_progress(0.6);

        let page_count = metadata.page_count;
        let title = metadata.title.as_ref();
        let author = metadata.author.as_ref();
        let subject = metadata.subject.as_ref();
        let creator = metadata.creator.as_ref();

        if let Some(title) = title {
            if object.metadata.title.is_none() {
                object.metadata.title = Some(title.clone());
            }
        }

        if let Some(author) = author {
            if object.metadata.authors.is_empty() {
                object.metadata.authors = vec![author.clone()];
            }
        }

        if let Some(desc) = subject {
            if object.metadata.description.is_none() {
                object.metadata.description = Some(desc.clone());
            }
        }

        object.custom_properties.insert(
            "pdf_page_count".to_string(),
            CustomPropertyValue::Number(page_count as f64),
        );

        object.custom_properties.insert(
            "pdf_metadata_extracted".to_string(),
            CustomPropertyValue::Text("true".to_string()),
        );

        let info = serde_json::json!({
            "page_count": page_count,
            "title": title,
            "author": author,
            "subject": subject,
            "creator": creator,
            "metadata_extracted": true,
            "warning": serde_json::Value::Null,
        });
        object.custom_properties.insert(
            "pdf_info".to_string(),
            CustomPropertyValue::Text(info.to_string()),
        );

        progress.set_progress(1.0);
        ProcessingResult::new(object).with_stats(
            ProcessingStats::new()
                .with_duration_ms(duration_ms)
                .with_metric("pdf_pages".to_string(), metadata.page_count.to_string()),
        )
    }

    fn supports(&self, object_type: &ObjectType) -> bool {
        matches!(object_type, ObjectType::Document)
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
    async fn test_pdf_metadata_real() {
        let obj = KnowledgeObject::new(
            ObjectType::Document,
            ObjectContent::Binary {
                mime_type: "application/pdf".to_string(),
                data: fixture_pdf(),
                filename: Some("report.pdf".to_string()),
            },
        );

        let ctx = ProcessingContext::new(obj);
        let processor = PdfMetadataProcessor;
        let result = processor
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        if cfg!(target_os = "macos") {
            assert!(result
                .object
                .custom_properties
                .contains_key("pdf_metadata_extracted"));
            let pages = result
                .object
                .custom_properties
                .get("pdf_page_count")
                .map(|v| match v {
                    crate::models::CustomPropertyValue::Number(n) => *n,
                    _ => -1.0,
                })
                .unwrap_or(-1.0);
            assert_eq!(pages, 1.0);
            assert!(result.object.metadata.title.is_some());

            // pdf_info JSON must be stored
            let pdf_info = result
                .object
                .custom_properties
                .get("pdf_info")
                .and_then(|v| match v {
                    crate::models::CustomPropertyValue::Text(s) => {
                        serde_json::from_str::<serde_json::Value>(s).ok()
                    }
                    _ => None,
                });
            assert!(pdf_info.is_some(), "pdf_info must be stored");
            let parsed = pdf_info.unwrap();
            assert!(
                parsed["metadata_extracted"].as_bool().unwrap_or(false),
                "metadata_extracted must be true"
            );
            assert!(
                parsed["page_count"].is_number(),
                "page_count must be a number"
            );
        } else {
            // Non-macOS: PlatformMicrophone unavailable, pdf_info warning is recorded.
            assert!(
                result.modified,
                "object should be modified to record platform warning"
            );
            let pdf_info = result
                .object
                .custom_properties
                .get("pdf_info")
                .and_then(|v| match v {
                    crate::models::CustomPropertyValue::Text(s) => {
                        serde_json::from_str::<serde_json::Value>(s).ok()
                    }
                    _ => None,
                });
            assert!(
                pdf_info.is_some(),
                "pdf_info must be stored even on failure"
            );
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
        let processor = PdfMetadataProcessor;
        let result = processor
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        assert!(!result.modified);
    }
}
