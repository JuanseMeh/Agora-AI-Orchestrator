use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaClientError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("media service error {status}: {message}")]
    ServiceError { status: u16, message: String },

    #[error("configuration error: {0}")]
    Configuration(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchProcessItem {
    pub media_id: String,
    pub text: String,
    pub content_type: String,
    pub mime_type: String,
    pub original_filename: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchProcessResponse {
    pub results: Vec<BatchProcessItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchProcessRequest {
    media_ids: Vec<String>,
}

/// HTTP client for the Media Service.
///
/// Reads `MEDIA_SERVICE_URL` from the environment at construction.
/// Provides methods for batch-processing files to extract text content.
pub struct MediaServiceClient {
    client: Client,
    base_url: String,
}

impl MediaServiceClient {
    pub fn from_env() -> Result<Self, MediaClientError> {
        let base_url = std::env::var("MEDIA_SERVICE_URL").map_err(|_| {
            MediaClientError::Configuration(
                "MEDIA_SERVICE_URL environment variable not set".to_string(),
            )
        })?;

        Ok(Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Sends a list of media IDs to the media service for batch text extraction.
    ///
    /// Returns processed text for each file that was found and successfully
    /// processed (OCR, transcription, or document parsing). Files that fail
    /// or are not found are silently skipped.
    pub async fn batch_process(
        &self,
        media_ids: Vec<String>,
    ) -> Result<Vec<BatchProcessItem>, MediaClientError> {
        if media_ids.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/media/internal/batch-process", self.base_url);
        let payload = BatchProcessRequest { media_ids };

        let response = self.client.post(&url).json(&payload).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(MediaClientError::ServiceError {
                status: status.as_u16(),
                message: body,
            });
        }

        let resp = response.json::<BatchProcessResponse>().await?;
        Ok(resp.results)
    }
}
