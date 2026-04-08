use thiserror::Error;
use crate::domain::ports::llm_provider::LlmError;
use crate::integration::gemini::types::GeminiErrorDetail;

/// Gemini-specific error type.
///
/// Lives entirely inside `integrations::gemini` — nothing outside
/// this module ever sees it. All public-facing code works with `LlmError`.
/// The `From` implementation at the bottom is the only exit point.
#[derive(Debug, Error)]
pub enum GeminiError {
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("gemini api error {code}: {message}")]
    Api { code: u16, message: String },

    #[error("response schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("failed to deserialize response: {0}")]
    Deserialization(String),

    #[error("empty response — no candidates returned")]
    EmptyResponse,

    #[error("request timeout after {seconds}s")]
    Timeout { seconds: u64 },
}

impl GeminiError {
    /// Constructs an `Api` variant from a parsed Gemini error detail block.
    pub fn from_api_detail(detail: GeminiErrorDetail) -> Self {
        Self::Api {
            code: detail.code,
            message: detail.message,
        }
    }
}

/// The only exit point from this module's error space.
///
/// `client.rs` returns `LlmError` — this conversion happens
/// at the boundary so nothing upstream ever imports `GeminiError`.
impl From<GeminiError> for LlmError {
    fn from(e: GeminiError) -> Self {
        match e {
            GeminiError::Network(e) => LlmError::Network(e.to_string()),

            GeminiError::Configuration(msg) => LlmError::Unexpected(msg),

            GeminiError::Api { code, message } => {
                LlmError::ProviderError { status: code, message }
            }
            
            GeminiError::SchemaValidation(msg) => LlmError::SchemaValidation(msg),

            GeminiError::Deserialization(msg) => LlmError::ParseFailure(msg),

            GeminiError::EmptyResponse => {
                LlmError::ParseFailure("no candidates in response".to_string())
            }

            GeminiError::Timeout { seconds } => LlmError::Timeout { seconds },
        }
    }
}