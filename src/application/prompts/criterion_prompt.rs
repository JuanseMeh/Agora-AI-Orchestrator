use crate::models::grading::rubric::RubricCriterion;

/// Builds the evaluation prompt for a single rubric criterion.
///
/// This is the only place in the system that knows how to translate
/// a `RubricCriterion` + raw submission text into an LLM-ready string.
/// The output is passed directly to `LlmProvider::evaluate_criterion()`.
pub struct CriterionPromptBuilder;

impl CriterionPromptBuilder {
    /// Builds a rubric-constrained evaluation prompt.
    ///
    /// The prompt instructs the LLM to:
    /// - score strictly within the provided scoring levels
    /// - justify the score using the criterion description
    /// - return a structured JSON response matching `CriterionResult`
    pub fn build(criterion: &RubricCriterion, submission_text: &str) -> String {
        let scoring_levels = criterion.levels_as_prompt_context();
        let description = criterion
            .description
            .as_deref()
            .unwrap_or("No description provided.");

        format!(
            "You are an academic evaluator. Your task is to evaluate a student submission \
            against a single rubric criterion and return a structured JSON response.\n\n\
            ## Criterion\n\
            Name: {name}\n\
            Description: {description}\n\n\
            ## Scoring Levels\n\
            {scoring_levels}\n\n\
            ## Student Submission\n\
            {submission}\n\n\
            ## Instructions\n\
            - You MUST assign a score that exactly matches one of the scoring level values above.\n\
            - Do NOT invent scores outside the defined levels.\n\
            - Your feedback must reference the criterion description and justify why \
              the submission matches the assigned level.\n\
            - Keep feedback concise: 2-4 sentences.\n\n\
            ## Required JSON Response Format\n\
            {{\n\
              \"criterion_id\": \"{criterion_id}\",\n\
              \"criterion_name\": \"{name}\",\n\
              \"score\": <number matching one scoring level>,\n\
              \"max_score\": {max_score},\n\
              \"feedback\": \"<your justification>\",\n\
              \"matched_level\": \"<label of the matched scoring level>\"\n\
            }}",
            name           = criterion.name,
            description    = description,
            scoring_levels = scoring_levels,
            submission     = submission_text,
            criterion_id   = criterion.criterion_id,
            max_score      = criterion.max_score(),
        )
    }
}
