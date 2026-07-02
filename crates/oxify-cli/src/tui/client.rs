use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Summary of a workflow fetched from the API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
}

/// Summary of an execution fetched from the API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionSummary {
    pub id: String,
    pub workflow_id: String,
    pub status: String,
    pub started_at: String,
    pub duration_ms: Option<u64>,
}

/// Lightweight HTTP client that wraps the OxiFY REST API for the TUI.
pub struct TuiApiClient {
    base_url: String,
    http: oxihttp::HttpsClient,
}

impl TuiApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: oxihttp::Client::builder()
                .with_tls()
                .build_https()
                .expect("failed to build oxihttp HTTPS client for TUI API"),
        }
    }

    /// Fetch the list of workflows from the API.
    /// Returns an empty vec if the server is unreachable or returns a non-2xx response.
    pub async fn list_workflows(&self) -> Result<Vec<WorkflowSummary>> {
        let url = format!("{}/api/v1/workflows", self.base_url);
        let request = match self.http.get(&url) {
            Ok(req) => req,
            Err(e) => {
                tracing::warn!("Failed to build request for workflows: {}", e);
                return Ok(vec![]);
            }
        };
        let resp = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to reach API for workflows: {}", e);
                return Ok(vec![]);
            }
        };
        if resp.status().is_success() {
            let data: serde_json::Value = resp.body_json().await?;
            let arr = if data.is_array() {
                data
            } else {
                data["workflows"].clone()
            };
            Ok(serde_json::from_value(arr).unwrap_or_default())
        } else {
            tracing::warn!("API returned {} for workflows", resp.status());
            Ok(vec![])
        }
    }

    /// Fetch the list of executions from the API.
    /// Returns an empty vec if the server is unreachable or returns a non-2xx response.
    pub async fn list_executions(&self) -> Result<Vec<ExecutionSummary>> {
        let url = format!("{}/api/v1/executions", self.base_url);
        let request = match self.http.get(&url) {
            Ok(req) => req,
            Err(e) => {
                tracing::warn!("Failed to build request for executions: {}", e);
                return Ok(vec![]);
            }
        };
        let resp = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to reach API for executions: {}", e);
                return Ok(vec![]);
            }
        };
        if resp.status().is_success() {
            let data: serde_json::Value = resp.body_json().await?;
            let arr = if data.is_array() {
                data
            } else {
                data["executions"].clone()
            };
            Ok(serde_json::from_value(arr).unwrap_or_default())
        } else {
            tracing::warn!("API returned {} for executions", resp.status());
            Ok(vec![])
        }
    }
}
