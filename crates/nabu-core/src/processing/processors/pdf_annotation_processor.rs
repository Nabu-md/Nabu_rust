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
        .with_source("pdf_annotation_processor".to_string())
        .with_category(DiagnosticCategory::Metadata)
}

/// Processes PDF annotations (highlights, notes, stamps) via the native
/// PDFKit engine ([`crate::native::pdfkit`]). No simulated annotations.
///
/// Extracted annotations are stored as custom properties:
/// - `annotation_count` — number of real annotations found
/// - `annotations_processed` — `"true"` when the engine ran
/// - `pdf_annotations` — JSON array of `{kind, contents, page}` entries
pub struct PdfAnnotationProcessor;

#[async_trait]
impl Processor for PdfAnnotationProcessor {
    fn name(&self) -> &'static str {
        "pdf_annotation_processor"
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

        let engine_result = tokio::task::spawn_blocking(move || {
            crate::native::pdfkit::extract_annotations(&pdf_data)
        })
        .await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let annotations = match engine_result {
            Ok(Ok(anns)) => anns,
            Ok(Err(NativeError::UnsupportedPlatform)) => {
                let info = serde_json::json!({
                    "annotation_count": 0,
                    "annotations_extracted": false,
                    "warning": "PDFKit unavailable on this platform (requires macOS)",
                });
                object.custom_properties.insert(
                    "pdf_annotation_info".to_string(),
                    crate::models::CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diag(
                        DiagnosticSeverity::Information,
                        "PDFKit unavailable on this platform; PDF preserved without annotation extraction".to_string(),
                        "PDF_PLATFORM_UNSUPPORTED",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("pdf_annotations".to_string(), "0".to_string()),
                    );
            }
            Ok(Err(e)) => {
                let err_msg = format!("PDFKit annotation extraction error: {e}");
                let info = serde_json::json!({
                    "annotation_count": 0,
                    "annotations_extracted": false,
                    "warning": err_msg,
                });
                object.custom_properties.insert(
                    "pdf_annotation_info".to_string(),
                    crate::models::CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diag(
                        DiagnosticSeverity::Error,
                        format!("PDF annotation extraction failed: {e}"),
                        "PDF_ENGINE_ERROR",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("pdf_annotations".to_string(), "0".to_string()),
                    );
            }
            Err(_) => {
                let info = serde_json::json!({
                    "annotation_count": 0,
                    "annotations_extracted": false,
                    "warning": "PDF annotation extraction task panicked",
                });
                object.custom_properties.insert(
                    "pdf_annotation_info".to_string(),
                    crate::models::CustomPropertyValue::Text(info.to_string()),
                );
                return ProcessingResult::new(object)
                    .add_diagnostic(pdf_diag(
                        DiagnosticSeverity::Critical,
                        "PDF annotation extraction task panicked".to_string(),
                        "PDF_PANIC",
                    ))
                    .with_stats(
                        ProcessingStats::new()
                            .with_duration_ms(duration_ms)
                            .with_metric("pdf_annotations".to_string(), "0".to_string()),
                    );
            }
        };

        object.custom_properties.insert(
            "annotation_count".to_string(),
            CustomPropertyValue::Number(annotations.len() as f64),
        );

        object.custom_properties.insert(
            "annotations_processed".to_string(),
            CustomPropertyValue::Text("true".to_string()),
        );

        if !annotations.is_empty() {
            let json = serde_json::json!({
                "annotations": annotations.iter().map(|a| {
                    serde_json::json!({
                        "kind": a.kind,
                        "contents": a.contents,
                        "page": a.page,
                    })
                }).collect::<Vec<_>>()
            });
            object.custom_properties.insert(
                "pdf_annotations".to_string(),
                CustomPropertyValue::Text(json.to_string()),
            );
        }

        let info = serde_json::json!({
            "annotation_count": annotations.len(),
            "annotations_extracted": true,
            "warning": serde_json::Value::Null,
        });
        object.custom_properties.insert(
            "pdf_annotation_info".to_string(),
            CustomPropertyValue::Text(info.to_string()),
        );

        progress.set_progress(1.0);
        ProcessingResult::new(object).with_stats(
            ProcessingStats::new()
                .with_duration_ms(duration_ms)
                .with_metric("pdf_annotations".to_string(), annotations.len().to_string()),
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
    async fn test_pdf_annotation_processing_real() {
        let obj = KnowledgeObject::new(
            ObjectType::Document,
            ObjectContent::Binary {
                mime_type: "application/pdf".to_string(),
                data: fixture_pdf(),
                filename: Some("annotated.pdf".to_string()),
            },
        );

        let ctx = ProcessingContext::new(obj);
        let processor = PdfAnnotationProcessor;
        let result = processor
            .process(&ctx, ProgressReporter::noop(), CancellationToken::new())
            .await;

        if cfg!(target_os = "macos") {
            assert_eq!(
                result
                    .object
                    .custom_properties
                    .get("annotations_processed")
                    .map(|v| match v {
                        crate::models::CustomPropertyValue::Text(s) => s.as_str(),
                        _ => "",
                    }),
                Some("true")
            );
            // pdf_annotation_info JSON must be stored
            let info = result
                .object
                .custom_properties
                .get("pdf_annotation_info")
                .and_then(|v| match v {
                    crate::models::CustomPropertyValue::Text(s) => {
                        serde_json::from_str::<serde_json::Value>(s).ok()
                    }
                    _ => None,
                });
            assert!(info.is_some(), "pdf_annotation_info must be stored");
            assert!(
                info.as_ref().unwrap()["annotations_extracted"]
                    .as_bool()
                    .unwrap_or(false),
                "annotations_extracted must be true"
            );
        } else {
            // Non-macOS: PDFKit unavailable, pdf_annotation_info warning is recorded.
            assert!(
                result.modified,
                "object should be modified to record platform warning"
            );
            let info = result
                .object
                .custom_properties
                .get("pdf_annotation_info")
                .and_then(|v| match v {
                    crate::models::CustomPropertyValue::Text(s) => {
                        serde_json::from_str::<serde_json::Value>(s).ok()
                    }
                    _ => None,
                });
            assert!(
                info.is_some(),
                "pdf_annotation_info must be stored even on failure"
            );
            let warning = info.and_then(|v| v["warning"].as_str().map(|s| s.to_string()));
            assert!(warning.is_some(), "warning must be set on non-macOS");
        }
    }
}
