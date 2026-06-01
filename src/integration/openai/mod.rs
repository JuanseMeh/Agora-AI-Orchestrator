use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::{error, info};

use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::models::grading::grading_result::CriterionResult;

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.groq.com/openai/v1";
const DEFAULT_OPENAI_MODEL: &str = "mixtral-8x7b-32768";

#[derive(Debug, Error)]
pub enum OpenAiError {
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("api error {code}: {message}")]
    Api { code: u16, message: String },

    #[error("failed to deserialize response: {0}")]
    Deserialization(String),

    #[error("empty response — no choices returned")]
    EmptyResponse,
}

impl From<OpenAiError> for LlmError {
    fn from(e: OpenAiError) -> Self {
        match e {
            OpenAiError::Network(e) => LlmError::Network(e.to_string()),
            OpenAiError::Configuration(msg) => LlmError::Unexpected(msg),
            OpenAiError::Api { code, message } => LlmError::ProviderError { status: code, message },
            OpenAiError::Deserialization(msg) => LlmError::ParseFailure(msg),
            OpenAiError::EmptyResponse => LlmError::ParseFailure("no choices in response".to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorDetail {
    message: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    #[allow(unused)]
    r#type: Option<String>,
}

pub struct OpenAiClient {
    client: Client,
    api_key: String,
    pub model: String,
    base_url: String,
}

impl OpenAiClient {
    pub fn from_env() -> Result<Self, OpenAiError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| OpenAiError::Configuration(
                "OPENAI_API_KEY environment variable not set".to_string()
            ))?;

        if api_key.trim().is_empty() {
            return Err(OpenAiError::Configuration(
                "OPENAI_API_KEY is empty".to_string()
            ));
        }

        let model = std::env::var("OPENAI_MODEL")
            .ok()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_OPENAI_MODEL.to_string());

        let base_url = std::env::var("OPENAI_BASE_URL")
            .ok()
            .map(|m| m.trim().trim_end_matches('/').to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string());

        let client = Client::builder()
            .no_proxy()
            .http1_only()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| OpenAiError::Configuration(format!("failed to build http client: {}", e)))?;

        Ok(Self { client, api_key, model, base_url })
    }

    fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    async fn send(&self, prompt: String) -> Result<CriterionResult, OpenAiError> {
        info!(
            model = %self.model,
            prompt_len = prompt.len(),
            "openai send — calling chat completions"
        );

        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "system".to_string(),
                content: "You are a grading assistant. Respond only with valid JSON matching the requested schema.".to_string(),
            }, Message {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.0,
            response_format: Some(ResponseFormat {
                format_type: "json_object".to_string(),
            }),
        };

        let response = self
            .client
            .post(self.chat_completions_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
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
                    "openai request failed"
                );
                OpenAiError::Network(e)
            })?;

        let status = response.status();

        if !status.is_success() {
            let error_body = response
                .json::<OpenAiErrorResponse>()
                .await
                .map_err(|e| {
                    error!(error = %e, status = %status, "failed to parse openai api error response");
                    OpenAiError::Deserialization(e.to_string())
                })?;

            let err_detail = error_body.error;
            let code = err_detail.code
                .as_ref()
                .and_then(|c| c.parse::<u16>().ok())
                .unwrap_or(status.as_u16());

            error!(
                status = %status,
                error_detail = ?err_detail,
                "openai send — API error"
            );
            return Err(OpenAiError::Api {
                code,
                message: err_detail.message,
            });
        }

        let completion = response
            .json::<ChatCompletionResponse>()
            .await
            .map_err(|e| OpenAiError::Deserialization(e.to_string()))?;

        let finish_reasons: Vec<Option<String>> = completion
            .choices
            .iter()
            .map(|c| c.finish_reason.clone())
            .collect();

        let text = completion
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or(OpenAiError::EmptyResponse)?;

        info!(
            "openai send — success finish_reasons={:?} text_len={}",
            finish_reasons,
            text.len(),
        );

        serde_json::from_str::<CriterionResult>(&text)
            .map_err(|e| OpenAiError::Deserialization(e.to_string()))
    }

    async fn send_text(&self, prompt: String) -> Result<String, OpenAiError> {
        info!(
            model = %self.model,
            prompt_len = prompt.len(),
            "openai send_text — calling chat completions"
        );

        let request_body = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.0,
            response_format: None,
        };

        let response = self
            .client
            .post(self.chat_completions_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
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
                    "openai send_text request failed"
                );
                OpenAiError::Network(e)
            })?;

        let status = response.status();

        if !status.is_success() {
            let error_body = response
                .json::<OpenAiErrorResponse>()
                .await
                .map_err(|e| {
                    error!(error = %e, status = %status, "failed to parse openai api error response");
                    OpenAiError::Deserialization(e.to_string())
                })?;

            let err_detail = error_body.error;
            let code = err_detail.code
                .as_ref()
                .and_then(|c| c.parse::<u16>().ok())
                .unwrap_or(status.as_u16());

            error!(
                status = %status,
                error_detail = ?err_detail,
                "openai send_text — API error"
            );
            return Err(OpenAiError::Api {
                code,
                message: err_detail.message,
            });
        }

        let completion = response
            .json::<ChatCompletionResponse>()
            .await
            .map_err(|e| OpenAiError::Deserialization(e.to_string()))?;

        let finish_reasons: Vec<Option<String>> = completion
            .choices
            .iter()
            .map(|c| c.finish_reason.clone())
            .collect();

        let text = completion
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or(OpenAiError::EmptyResponse)?;

        info!(
            "openai send_text — success finish_reasons={:?} text_len={}",
            finish_reasons,
            text.len(),
        );

        Ok(text)
    }
}

#[async_trait]
impl LlmProvider for OpenAiClient {
    async fn evaluate_criterion(
        &self,
        prompt: String,
    ) -> Result<CriterionResult, LlmError> {
        self.send(prompt).await.map_err(LlmError::from)
    }

    async fn generate_text(
        &self,
        prompt: String,
    ) -> Result<String, LlmError> {
        self.send_text(prompt).await.map_err(LlmError::from)
    }
}
