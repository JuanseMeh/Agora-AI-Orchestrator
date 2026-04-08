use serde::{Deserialize, Serialize};

/// Root request body sent to the Gemini generateContent endpoint.
///
/// POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent
#[derive(Debug, Serialize)]
pub struct GeminiRequest {
    pub contents: Vec<Content>,
    pub generation_config: GenerationConfig,
}

/// A single conversational turn in the request.
/// For single-shot evaluation we always send exactly one user Content.
#[derive(Debug, Serialize)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

/// The text body of a conversational turn.
#[derive(Debug, Serialize)]
pub struct Part {
    pub text: String,
}

/// Generation configuration — controls output format and constraints.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// Forces Gemini into JSON mode — response will be valid JSON.
    pub response_mime_type: String,
    /// Declares the exact JSON shape Gemini must conform to.
    pub response_schema: ResponseSchema,
}

/// The JSON schema Gemini uses to validate and structure its output.
/// Mirrors the fields of `CriterionResult` exactly.
#[derive(Debug, Serialize)]
pub struct ResponseSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: CriterionResultSchema,
    pub required: Vec<String>,
}

/// Field-level schema definitions for the CriterionResult shape.
#[derive(Debug, Serialize)]
pub struct CriterionResultSchema {
    pub criterion_id: SchemaField,
    pub criterion_name: SchemaField,
    pub score: SchemaField,
    pub max_score: SchemaField,
    pub feedback: SchemaField,
    pub matched_level: SchemaField,
}

/// A single field descriptor within a response schema.
#[derive(Debug, Serialize)]
pub struct SchemaField {
    #[serde(rename = "type")]
    pub field_type: String,
    pub description: String,
}

/// Root response body returned by Gemini.
#[derive(Debug, Deserialize)]
pub struct GeminiResponse {
    pub candidates: Vec<Candidate>,
}

/// A single generated candidate from Gemini.
/// Under normal operation there is always exactly one.
#[derive(Debug, Deserialize)]
pub struct Candidate {
    pub content: CandidateContent,
    pub finish_reason: Option<String>,
}

/// The content block inside a candidate.
#[derive(Debug, Deserialize)]
pub struct CandidateContent {
    pub parts: Vec<CandidatePart>,
}

/// The text payload inside a candidate content block.
#[derive(Debug, Deserialize)]
pub struct CandidatePart {
    pub text: String,
}

/// Gemini API error response body.
/// Returned when the HTTP status is not 2xx.
#[derive(Debug, Deserialize)]
pub struct GeminiErrorResponse {
    pub error: GeminiErrorDetail,
}

/// Detail block inside a Gemini error response.
#[derive(Debug, Deserialize)]
pub struct GeminiErrorDetail {
    pub code: u16,
    pub message: String,
    pub status: String,
}

impl GeminiRequest {
    /// Constructs a single-turn evaluation request with JSON mode enabled.
    /// Called by `request_builder.rs` — never constructed manually elsewhere.
    pub fn new(prompt: String, schema: ResponseSchema) -> Self {
        Self {
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part { text: prompt }],
            }],
            generation_config: GenerationConfig {
                response_mime_type: "application/json".to_string(),
                response_schema: schema,
            },
        }
    }
}

impl GeminiResponse {
    /// Extracts the raw text from the first candidate's first part.
    /// Returns None if the response structure is empty or malformed.
    pub fn extract_text(&self) -> Option<&str> {
        self.candidates
            .first()?
            .content
            .parts
            .first()
            .map(|p| p.text.as_str())
    }
}