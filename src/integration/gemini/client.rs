use async_trait::async_trait;
use reqwest::Client;
use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::models::grading::grading_result::CriterionResult;
use crate::integration::gemini::error::GeminiError;
use crate::integration::gemini::request_builder::RequestBuilder;
use crate::integration::gemini::response_parser::ResponseParser;
use crate::integration::gemini::types::{GeminiErrorResponse, GeminiResponse};

const GEMINI_API_BASE: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent";

/// Gemini implementation of `LlmProvider`.
///
/// API key is read once from the environment at construction time.
/// Stateless beyond that — safe to wrap in `Arc` and share across tasks.
pub struct GeminiClient {
    client: Client,
    api_key: String,
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

        Ok(Self {
            client: Client::new(),
            api_key,
        })
    }

    fn endpoint_url(&self) -> String {
        format!("{}?key={}", GEMINI_API_BASE, self.api_key)
    }

    async fn send(&self, prompt: String) -> Result<CriterionResult, GeminiError> {
        let request_body = RequestBuilder::build(prompt);

        let response = self
            .client
            .post(self.endpoint_url())
            .json(&request_body)
            .send()
            .await?;

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

        ResponseParser::parse(gemini_response)
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
}