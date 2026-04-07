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