/// HTTP client for the Raidhub API.
/// Handles sync and status endpoints with Bearer token auth.

use crate::bis_sync::ObtainedItemUpdate;
use crate::gargul_parser::GargulSyncPayload;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BASE_URL: &str = "https://www.goldschweine.de";

#[derive(Debug)]
pub enum ApiError {
    Unauthorized(String),
    BadRequest(String),
    ServerError(String),
    NetworkError(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unauthorized(detail) => {
                if detail.is_empty() {
                    write!(f, "Token expired or revoked")
                } else {
                    write!(f, "Token expired or revoked ({})", detail)
                }
            }
            ApiError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            ApiError::ServerError(msg) => write!(f, "Server error: {}", msg),
            ApiError::NetworkError(msg) => write!(f, "Network error: {}", msg),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub synced: u32,
    pub updated: u32,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "discordUsername")]
    pub discord_username: Option<String>,
    #[serde(rename = "avatarUrl")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastSyncInfo {
    pub timestamp: String,
    pub imported: u32,
    pub updated: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub connected: bool,
    #[serde(rename = "authMethod")]
    pub auth_method: String,
    pub user: UserInfo,
    #[serde(rename = "lastSync")]
    pub last_sync: Option<LastSyncInfo>,
    #[serde(rename = "characterCount")]
    pub character_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GdkpSyncResponse {
    pub sessions_imported: u32,
    pub sessions_updated: u32,
    pub items_imported: u32,
    #[serde(default)]
    pub import_id: Option<String>,
}

pub struct RaidhubApiClient {
    client: Client,
    token: String,
}

impl RaidhubApiClient {
    pub fn new(token: &str) -> Self {
        Self {
            client: Client::new(),
            token: token.to_string(),
        }
    }

    /// POST /api/chartracker/sync — sync character data
    pub async fn sync_characters(&self, export_data: &Value) -> Result<SyncResponse, ApiError> {
        let body = serde_json::json!({
            "exportData": export_data
        });

        let response = self
            .client
            .post(format!("{}/api/chartracker/sync", BASE_URL))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::NetworkError(e.to_string()))?;

        match response.status().as_u16() {
            200 => response
                .json::<SyncResponse>()
                .await
                .map_err(|e| ApiError::ServerError(format!("Failed to parse response: {}", e))),
            401 => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::Unauthorized(text))
            }
            400 => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::BadRequest(text))
            }
            status => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::ServerError(format!("HTTP {}: {}", status, text)))
            }
        }
    }

    /// POST /api/gdkp/gargul/sync — sync GDKP session data from Gargul
    pub async fn sync_gdkp(
        &self,
        payload: &GargulSyncPayload,
    ) -> Result<GdkpSyncResponse, ApiError> {
        let response = self
            .client
            .post(format!("{}/api/gdkp/gargul/sync", BASE_URL))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(payload)
            .send()
            .await
            .map_err(|e| ApiError::NetworkError(e.to_string()))?;

        match response.status().as_u16() {
            200 => response
                .json::<GdkpSyncResponse>()
                .await
                .map_err(|e| ApiError::ServerError(format!("Failed to parse response: {}", e))),
            401 => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::Unauthorized(text))
            }
            400 => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::BadRequest(text))
            }
            status => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::ServerError(format!("HTTP {}: {}", status, text)))
            }
        }
    }

    /// GET /api/chartracker/bis-export — download BiS lists for addon
    pub async fn get_bis_export(&self) -> Result<Value, ApiError> {
        let response = self
            .client
            .get(format!("{}/api/chartracker/bis-export", BASE_URL))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| ApiError::NetworkError(e.to_string()))?;

        match response.status().as_u16() {
            200 => response
                .json::<Value>()
                .await
                .map_err(|e| ApiError::ServerError(format!("Failed to parse response: {}", e))),
            401 => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::Unauthorized(text))
            }
            status => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::ServerError(format!("HTTP {}: {}", status, text)))
            }
        }
    }

    /// POST /api/chartracker/bis-sync — sync obtained items back to website
    pub async fn sync_bis_obtained(
        &self,
        items: &[ObtainedItemUpdate],
    ) -> Result<Value, ApiError> {
        let body = serde_json::json!({ "obtained_items": items });

        let response = self
            .client
            .post(format!("{}/api/chartracker/bis-sync", BASE_URL))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::NetworkError(e.to_string()))?;

        match response.status().as_u16() {
            200 => response
                .json::<Value>()
                .await
                .map_err(|e| ApiError::ServerError(format!("Failed to parse response: {}", e))),
            401 => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::Unauthorized(text))
            }
            400 => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::BadRequest(text))
            }
            status => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::ServerError(format!("HTTP {}: {}", status, text)))
            }
        }
    }

    /// GET /api/chartracker/sync/status — check connection and get user info
    pub async fn get_status(&self) -> Result<StatusResponse, ApiError> {
        let response = self
            .client
            .get(format!("{}/api/chartracker/sync/status", BASE_URL))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| ApiError::NetworkError(e.to_string()))?;

        match response.status().as_u16() {
            200 => response
                .json::<StatusResponse>()
                .await
                .map_err(|e| ApiError::ServerError(format!("Failed to parse response: {}", e))),
            401 => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::Unauthorized(text))
            }
            status => {
                let text = response.text().await.unwrap_or_default();
                Err(ApiError::ServerError(format!("HTTP {}: {}", status, text)))
            }
        }
    }
}
