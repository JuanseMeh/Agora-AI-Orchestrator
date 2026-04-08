use crate::integration::gemini::error::GeminiError;
use crate::integration::gemini::types::GeminiResponse;
use crate::models::grading::grading_result::CriterionResult;

/// Extracts and deserializes a `CriterionResult` from a raw `GeminiResponse`.
///
/// This is the only place in the system that knows how to navigate
/// Gemini's response envelope and produce a typed domain model from it.
pub struct ResponseParser;

impl ResponseParser {
    /// Parses a `GeminiResponse` into a `CriterionResult`.
    ///
    /// Fails with `GeminiError::EmptyResponse` if no candidates were returned.
    /// Fails with `GeminiError::Deserialization` if the JSON text cannot be
    /// mapped onto `CriterionResult`.
    pub fn parse(response: GeminiResponse) -> Result<CriterionResult, GeminiError> {
        let text = response
            .extract_text()
            .ok_or(GeminiError::EmptyResponse)?;

        serde_json::from_str::<CriterionResult>(text)
            .map_err(|e| GeminiError::Deserialization(e.to_string()))
    }
}