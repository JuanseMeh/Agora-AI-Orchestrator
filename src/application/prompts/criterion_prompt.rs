use crate::models::embedding::SimilarGradeExample;
use crate::models::grading::rubric::RubricCriterion;

/// Builds the evaluation prompt for a single rubric criterion.
///
/// This is the only place in the system that knows how to translate
/// a `RubricCriterion` + submission data into an LLM-ready string.
/// The output is passed directly to `LlmProvider::evaluate_criterion()`.
pub struct CriterionPromptBuilder;

impl CriterionPromptBuilder {
    /// Builds a rubric-constrained evaluation prompt with basic context.
    pub fn build(criterion: &RubricCriterion, submission_text: &str) -> String {
        let scoring_levels = criterion.levels_as_prompt_context();
        let description = criterion
            .description
            .as_deref()
            .unwrap_or("No description provided.");

        Self::format_prompt(
            criterion,
            submission_text,
            &scoring_levels,
            &description,
            &[],
            None,
            None,
            None,
            None,
        )
    }

    /// Builds an evaluation prompt augmented with similar past grading
    /// examples retrieved via RAG (vector similarity search).
    pub fn build_with_rag(
        criterion: &RubricCriterion,
        submission_text: &str,
        examples: &[SimilarGradeExample],
    ) -> String {
        let scoring_levels = criterion.levels_as_prompt_context();
        let description = criterion
            .description
            .as_deref()
            .unwrap_or("No description provided.");

        Self::format_prompt(
            criterion,
            submission_text,
            &scoring_levels,
            &description,
            examples,
            None,
            None,
            None,
            None,
        )
    }

    /// Builds an evaluation prompt with full assignment context, including
    /// title, description, reference attachment content, and submission files.
    pub fn build_with_full_context(
        criterion: &RubricCriterion,
        submission_text: &str,
        assignment_title: &str,
        assignment_description: &str,
        assignment_attachments: Option<&str>,
        submission_files: Option<&str>,
        examples: &[SimilarGradeExample],
    ) -> String {
        let scoring_levels = criterion.levels_as_prompt_context();
        let description = criterion
            .description
            .as_deref()
            .unwrap_or("No description provided.");

        Self::format_prompt(
            criterion,
            submission_text,
            &scoring_levels,
            &description,
            examples,
            Some(assignment_title),
            Some(assignment_description),
            assignment_attachments,
            submission_files,
        )
    }

    fn format_prompt(
        criterion: &RubricCriterion,
        submission_text: &str,
        scoring_levels: &str,
        description: &str,
        examples: &[SimilarGradeExample],
        assignment_title: Option<&str>,
        assignment_description: Option<&str>,
        assignment_attachments: Option<&str>,
        submission_files: Option<&str>,
    ) -> String {
        let mut prompt = String::new();

        // ── Assignment context ─────────────────────────────────────────
        if let Some(title) = assignment_title {
            prompt.push_str("## Assignment Context\n");
            prompt.push_str(&format!("Title: {}\n", title));
            if let Some(desc) = assignment_description {
                if !desc.is_empty() {
                    prompt.push_str(&format!("Description: {}\n", desc));
                }
            }
            prompt.push('\n');
        }

        // ── Reference documents (assignment attachments) ───────────────
        if let Some(attachments) = assignment_attachments {
            prompt.push_str(
                "## Reference Documents (attached to the assignment)\n\
                 Below is the content extracted from reference documents attached \
                 to this assignment. Use it to evaluate criteria that require \
                 comparison with the assignment materials.\n\n",
            );
            prompt.push_str(attachments);
            prompt.push_str("\n\n");
        }

        // ── Submission attachments ──────────────────────────────────────
        if let Some(files) = submission_files {
            prompt.push_str(
                "## Submission Attachments\n\
                 Below is the text content extracted from files the student \
                 attached to their submission. Use it alongside the submission \
                 text to evaluate the student's work.\n\n",
            );
            prompt.push_str(files);
            prompt.push_str("\n\n");
        }

        // ── Criterion ───────────────────────────────────────────────────
        prompt.push_str(&format!(
            "## Criterion\n\
             Name: {name}\n\
             Description: {description}\n\n\
             ## Scoring Levels\n\
             {scoring_levels}\n\n",
            name = criterion.name,
            description = description,
            scoring_levels = scoring_levels,
        ));

        // ── RAG examples ────────────────────────────────────────────────
        if !examples.is_empty() {
            prompt.push_str(
                "## Similar Past Grades (Reference Examples)\n\
                 The following are examples of how similar submissions were graded \
                 for this criterion. Use them to calibrate your scoring consistency, \
                 but evaluate this submission on its own merit.\n\n",
            );

            for (i, ex) in examples.iter().enumerate() {
                prompt.push_str(&format!(
                    "### Example {}\n\
                     - Score: {:.1} / {:.1}\n\
                     - Matched Level: {}\n\
                     - Feedback: {}\n\
                     - Similarity to current submission: {:.0}%\n\n",
                    i + 1,
                    ex.score,
                    ex.max_score,
                    ex.matched_level,
                    ex.feedback,
                    ex.similarity_score * 100.0,
                ));
            }
        }

        // ── Student submission ──────────────────────────────────────────
        prompt.push_str(&format!(
            "## Student Submission\n\
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
            submission = submission_text,
            criterion_id = criterion.criterion_id,
            name = criterion.name,
            max_score = criterion.max_score(),
        ));

        prompt
    }
}
