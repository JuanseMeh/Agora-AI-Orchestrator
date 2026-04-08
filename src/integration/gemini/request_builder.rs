use crate::integration::gemini::types::{
    CriterionResultSchema, GeminiRequest, ResponseSchema, SchemaField,
};

/// Assembles the full Gemini request payload from a prompt string.
///
/// This is the only place in the system that knows about Gemini's
/// `response_schema` structure. The prompt string comes in already
/// built by `application::prompts::criterion_prompt`.
pub struct RequestBuilder;

impl RequestBuilder {
    /// Builds a `GeminiRequest` with JSON mode enabled and the
    /// `CriterionResult` schema wired into `generation_config`.
    pub fn build(prompt: String) -> GeminiRequest {
        let schema = Self::criterion_result_schema();
        GeminiRequest::new(prompt, schema)
    }

    /// Constructs the response schema that mirrors `CriterionResult`.
    ///
    /// This schema is what forces Gemini to produce a valid, structured
    /// JSON response. Field types and descriptions must stay in sync
    /// with `CriterionResult` in `models::grading::grading_result`.
    fn criterion_result_schema() -> ResponseSchema {
        ResponseSchema {
            schema_type: "object".to_string(),
            required: vec![
                "criterion_id".to_string(),
                "criterion_name".to_string(),
                "score".to_string(),
                "max_score".to_string(),
                "feedback".to_string(),
                "matched_level".to_string(),
            ],
            properties: CriterionResultSchema {
                criterion_id: SchemaField {
                    field_type: "string".to_string(),
                    description: "The unique identifier of the rubric criterion.".to_string(),
                },
                criterion_name: SchemaField {
                    field_type: "string".to_string(),
                    description: "The display name of the rubric criterion.".to_string(),
                },
                score: SchemaField {
                    field_type: "number".to_string(),
                    description: "Score awarded, must exactly match one of the rubric scoring level values.".to_string(),
                },
                max_score: SchemaField {
                    field_type: "number".to_string(),
                    description: "Maximum achievable score for this criterion.".to_string(),
                },
                feedback: SchemaField {
                    field_type: "string".to_string(),
                    description: "2-4 sentence justification referencing the rubric level matched.".to_string(),
                },
                matched_level: SchemaField {
                    field_type: "string".to_string(),
                    description: "The label of the scoring level that was matched.".to_string(),
                },
            },
        }
    }
}