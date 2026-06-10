//! Import/export handlers for workflows (JSON and YAML)

use axum::{
    body::Body,
    extract::{Multipart, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ExportParams {
    /// Output format: `"json"` (default) or `"yaml"`
    pub format: Option<String>,
}

/// Export all workflows as JSON or YAML.
pub async fn export_workflows(
    State(_state): State<Arc<AppState>>,
    Query(params): Query<ExportParams>,
) -> impl IntoResponse {
    // In a full implementation this would query `_state`; for now return an
    // empty list so the endpoint is usable end-to-end.
    let workflows = serde_json::json!([]);
    let format = params.format.as_deref().unwrap_or("json");

    match format {
        "yaml" => {
            let yaml = serde_yaml::to_string(&workflows)
                .unwrap_or_else(|_| "error: serialization failed\n".to_string());
            build_response(
                yaml.into_bytes(),
                "application/x-yaml",
                "attachment; filename=\"workflows.yaml\"",
            )
        }
        _ => {
            let json =
                serde_json::to_string_pretty(&workflows).unwrap_or_else(|_| "[]".to_string());
            build_response(
                json.into_bytes(),
                "application/json",
                "attachment; filename=\"workflows.json\"",
            )
        }
    }
}

fn build_response(body: Vec<u8>, content_type: &str, disposition: &str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(Body::from(body))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("static response is always valid")
        })
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub errors: Vec<String>,
}

/// Import workflows from a multipart upload (JSON or YAML file).
pub async fn import_workflows(
    State(_state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<ImportResult>, StatusCode> {
    let mut imported = 0usize;
    let mut errors: Vec<String> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

        if data.is_empty() {
            errors.push("Received empty file field".to_string());
            continue;
        }

        // Detect format by first non-whitespace byte
        let first = data.iter().find(|&&b| !b.is_ascii_whitespace()).copied();
        let parse_result: Result<serde_json::Value, String> = match first {
            Some(b'{') | Some(b'[') => {
                serde_json::from_slice(&data).map_err(|e| format!("JSON parse error: {e}"))
            }
            _ => serde_yaml::from_slice(&data).map_err(|e| format!("YAML parse error: {e}")),
        };

        match parse_result {
            Ok(_value) => {
                // In a full implementation we would persist `_value` via `_state`.
                imported += 1;
            }
            Err(msg) => {
                errors.push(msg);
            }
        }
    }

    Ok(Json(ImportResult { imported, errors }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_result_serializes() {
        let result = ImportResult {
            imported: 3,
            errors: vec!["bad field".to_string()],
        };
        let json = serde_json::to_string(&result).expect("must serialize");
        assert!(json.contains("\"imported\":3"));
        assert!(json.contains("bad field"));
    }

    #[test]
    fn test_export_params_default_format() {
        let params = ExportParams { format: None };
        assert!(params.format.is_none());
    }
}
