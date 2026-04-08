// application/workflows/grading/steps/evaluate_criteria.rs

use crate::application::prompts::criterion_prompt::CriterionPromptBuilder;
use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::models::grading::rubric::RubricCriterion;
use crate::models::grading::grading_result::CriterionResult;
use crate::models::context::submission_context::SubmissionContext;

/// Evaluates all rubric criteria for a single submission.
///
/// Iterates over every criterion independently, builds a prompt for each,
/// and delegates evaluation to the injected `LlmProvider`.
/// Criterion evaluation is sequential for MVP — parallelization comes later.
pub struct EvaluateCriteria<'a> {
    provider: &'a dyn LlmProvider,
}

impl<'a> EvaluateCriteria<'a> {
    pub fn new(provider: &'a dyn LlmProvider) -> Self {
        Self { provider }
    }

    /// Evaluates every criterion in `criteria` against the submission content.
    /// Returns a `Vec<CriterionResult>` in the same order as the input criteria.
    /// Fails fast on the first criterion that returns an error.
    pub async fn run(
        &self,
        criteria: &[RubricCriterion],
        submission: &SubmissionContext,
    ) -> Result<Vec<CriterionResult>, LlmError> {
        let mut results = Vec::with_capacity(criteria.len());

        for criterion in criteria {
            let prompt = CriterionPromptBuilder::build(criterion, &submission.content);
            let result = self.provider.evaluate_criterion(prompt).await?;
            results.push(result);
        }

        Ok(results)
    }
}