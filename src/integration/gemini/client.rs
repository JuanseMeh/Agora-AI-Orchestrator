use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;
use tracing::error;
use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::models::grading::grading_result::CriterionResult;
use crate::integration::gemini::error::GeminiError;
use crate::integration::gemini::request_builder::RequestBuilder;
use crate::integration::gemini::response_parser::ResponseParser;
use crate::integration::gemini::types::{GeminiErrorResponse, GeminiResponse};

const GEMINI_API_BASE: &str =
    "https://generativelanguage.googleapis.com/v1beta/models";
const DEFAULT_GEMINI_MODEL: &str = "gemini-2.0-flash";

/// Gemini implementation of `LlmProvider`.
///
/// API key is read once from the environment at construction time.
/// Stateless beyond that — safe to wrap in `Arc` and share across tasks.
pub struct GeminiClient {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiClient {
    /// Constructs a new `GeminiClient`, reading the API key from the
    /// `GEMINI_API_KEY` environment variable.
    ///
    /// Returns an error if the variable is missing or empty.
    pub fn from_env() -> Result<Self, GeminiError> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .map_err(|_| GeminiError::Configuration(
                "GEMINI_API_KEY environment variable not set".to_string()
            ))?;

        if api_key.trim().is_empty() {
            return Err(GeminiError::Configuration(
                "GEMINI_API_KEY is empty".to_string()
            ));
        }
        let model = std::env::var("GEMINI_MODEL")
            .ok()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.to_string());
        let client = Client::builder()
            .no_proxy()
            .http1_only()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .build()
            .map_err(|e| GeminiError::Configuration(format!("failed to build http client: {}", e)))?;

        Ok(Self {
            client,
            api_key,
            model,
        })
    }

    fn endpoint_url(&self) -> String {
        format!(
            "{}/{}:generateContent?key={}",
            GEMINI_API_BASE, self.model, self.api_key
        )
    }

    async fn send(&self, prompt: String) -> Result<CriterionResult, GeminiError> {
        let request_body = RequestBuilder::build(prompt);

        let response = self
            .client
            .post(self.endpoint_url())
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    is_connect = e.is_connect(),
                    is_timeout = e.is_timeout(),
                    is_request = e.is_request(),
                    is_body = e.is_body(),
                    "gemini request failed"
                );
                GeminiError::Network(e)
            })?;

        let status = response.status();

        if !status.is_success() {
            let error_body = response
                .json::<GeminiErrorResponse>()
                .await
                .map_err(|e| {
                    error!(error = %e, status = %status, "failed to parse gemini api error response");
                    GeminiError::Deserialization(e.to_string())
                })?;

            return Err(GeminiError::from_api_detail(error_body.error));
        }

        let gemini_response = response
            .json::<GeminiResponse>()
            .await
            .map_err(|e| GeminiError::Deserialization(e.to_string()))?;

        ResponseParser::parse(gemini_response)
    }

    async fn send_text(&self, prompt: String) -> Result<String, GeminiError> {
        tracing::debug!("gemini send_text prompt_len={}", prompt.len());

        let request_body = serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [{ "text": prompt }]
            }],
            "generationConfig": {
                "temperature": 0.3,
                "maxOutputTokens": 2000
            }
        });

        let response = self
            .client
            .post(self.endpoint_url())
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    is_connect = e.is_connect(),
                    is_timeout = e.is_timeout(),
                    is_request = e.is_request(),
                    is_body = e.is_body(),
                    "gemini text request failed"
                );
                GeminiError::Network(e)
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .json::<GeminiErrorResponse>()
                .await
                .map_err(|e| GeminiError::Deserialization(e.to_string()))?;
            return Err(GeminiError::from_api_detail(error_body.error));
        }

        let gemini_response = response
            .json::<GeminiResponse>()
            .await
            .map_err(|e| GeminiError::Deserialization(e.to_string()))?;

        // Log diagnostic info: finish reasons and returned text length
        let finish_reasons: Vec<Option<String>> = gemini_response
            .candidates
            .iter()
            .map(|c| c.finish_reason.clone())
            .collect();

        let text_opt = gemini_response.extract_text();
        tracing::debug!("gemini send_text finish_reasons={:?} text_len={}", finish_reasons, text_opt.map(|t| t.len()).unwrap_or(0));

        let text = text_opt.ok_or(GeminiError::EmptyResponse)?;

        Ok(text.to_string())
    }
}

#[async_trait]
impl LlmProvider for GeminiClient {
    async fn evaluate_criterion(
        &self,
        prompt: String,
    ) -> Result<CriterionResult, LlmError> {
        self.send(prompt)
            .await
            .map_err(LlmError::from)
    }

    async fn generate_text(
        &self,
        prompt: String,
    ) -> Result<String, LlmError> {
        self.send_text(prompt)
            .await
            .map_err(LlmError::from)
    }
}
