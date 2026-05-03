use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum UserConfigClientError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("user-service error {status}: {message}")]
    ServiceError { status: u16, message: String },
    #[error("failed to deserialize response: {0}")]
    Deserialization(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalAiProfileResponse {
    pub user_id: Uuid,
    pub agentic_mode: bool,
    pub retro_style: String,
    pub exigency_level: String,
    pub weekly_report: bool,
    pub send_email_notification: bool,
}

pub struct UserConfigClient {
    client: Client,
    base_url: String,
}

impl UserConfigClient {
    pub fn from_env() -> Result<Self, UserConfigClientError> {
        let base_url = std::env::var("USER_SERVICE_URL")
            .map_err(|_| UserConfigClientError::Configuration("USER_SERVICE_URL environment variable not set".to_string()))?;
        Ok(Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn fetch_ai_profile(&self, user_id: Uuid) -> Result<InternalAiProfileResponse, UserConfigClientError> {
        let url = format!("{}/internal/users/{}/ai-profile", self.base_url, user_id);
        let response = self.client.get(&url).send().await?;
        let status = response.status();

        if !status.is_success() {
            return Err(UserConfigClientError::ServiceError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        response
            .json::<InternalAiProfileResponse>()
            .await
            .map_err(|e| UserConfigClientError::Deserialization(e.to_string()))
    }
}
