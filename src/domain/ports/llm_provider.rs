use async_trait::async_trait;
use thiserror::Error;
use crate::models::grading::grading_result::CriterionResult;

/// The abstraction boundary between the orchestration layer
/// and any concrete LLM provider.
///
/// Any new provider (OpenAI, Anthropic, etc.) only needs to
/// implement this trait — nothing upstream changes.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Evaluates a single rubric criterion against a submission.
    ///
    /// Receives a fully-formed prompt string from `application::prompts`.
    /// Returns a structured `CriterionResult` or a provider-agnostic `LlmError`.
    async fn evaluate_criterion(
        &self,
        prompt: String,
    ) -> Result<CriterionResult, LlmError>;

    /// Generates free-form text for analytics/reporting tasks.
    async fn generate_text(
        &self,
        prompt: String,
    ) -> Result<String, LlmError>;

    /// Embeds a text string into a vector for semantic search.
    ///
    /// Returns a flat `Vec<f32>` embedding on success. The default
    /// implementation returns an error — override per-provider when
    /// an embedding API is available.
    async fn embed_text(
        &self,
        text: String,
    ) -> Result<Vec<f32>, LlmError> {
        let _ = text;
        Err(LlmError::Unexpected("embedding not supported by this provider".into()))
    }
}

/// Provider-agnostic error type.
///
/// Each integration maps its own error type into this enum.
/// The orchestration layer only ever sees `LlmError` — never `GeminiError`.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("network error: {0}")]
    Network(String),

    #[error("provider returned an error: {status} — {message}")]
    ProviderError { status: u16, message: String },

    #[error("response schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("failed to parse provider response: {0}")]
    ParseFailure(String),

    #[error("request timeout after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("unexpected error: {0}")]
    Unexpected(String),
}
