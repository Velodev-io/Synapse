use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const ELECTRON_PORT: u16 = 19223;

#[derive(Debug, Serialize, Deserialize)]
pub struct ElectronRequest {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub data: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ElectronResponse {
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(flatten)]
    pub other: Value,
}

pub async fn call_electron(
    action: &str,
    provider: Option<&str>,
    data: Value,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120)) // 2 min timeout for slow provider actions
        .build()
        .map_err(|e| e.to_string())?;

    let payload = ElectronRequest {
        action: action.to_string(),
        provider: provider.map(|p| p.to_string()),
        data,
    };

    let url = format!("http://127.0.0.1:{}/ipc", ELECTRON_PORT);
    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to Electron sidecar: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Electron sidecar returned status error: {}", res.status()));
    }

    let response_body = res
        .json::<ElectronResponse>()
        .await
        .map_err(|e| format!("Invalid JSON response from Electron sidecar: {}", e))?;

    if !response_body.success {
        return Err(response_body
            .error
            .unwrap_or_else(|| "Unknown error in Electron".to_string()));
    }

    Ok(response_body.other)
}
