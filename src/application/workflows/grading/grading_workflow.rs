// application/workflows/grading/grading_workflow.rs

use crate::application::workflows::steps::evaluate_criteria::EvaluateCriteria;
use crate::application::workflows::steps::aggregate_scores::AggregateScores;
use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::models::context::assignment_context::AssignmentContext;
use crate::models::context::submission_context::GradingContext;
use crate::models::grading::grading_result::GradingResult;

const GRADING_MODEL: &str = "gemini-2.0-flash";

/// Orchestrates the full grading pipeline for one assignment.
///
/// Receives a `GradingContext` containing one assignment and all its
/// pending submissions. Produces one `GradingResult` per submission.
///
/// This is a static deterministic pipeline — the sequence of steps
/// is fixed and does not involve LLM planning.
pub struct GradingWorkflow<'a> {
    provider: &'a dyn LlmProvider,
}

impl<'a> GradingWorkflow<'a> {
    pub fn new(provider: &'a dyn LlmProvider) -> Self {
        Self { provider }
    }

    /// Runs the grading pipeline for every submission in the context.
    ///
    /// Returns a `Vec<GradingResult>` — one per submission, in the same
    /// order as `context.submissions`.
    /// Fails fast if any submission's evaluation returns an error.
    pub async fn run(
        &self,
        assignment: &AssignmentContext,
        context: &GradingContext,
    ) -> Result<Vec<GradingResult>, LlmError> {
        let evaluator = EvaluateCriteria::new(self.provider);
        let mut results = Vec::with_capacity(context.submission_count());

        for submission in &context.submissions {
            let criteria_results = evaluator
                .run(&assignment.rubric.criteria, submission)
                .await?;

            let grading_result = AggregateScores::run(
                submission,
                &assignment.rubric,
                criteria_results,
                GRADING_MODEL,
            );

            results.push(grading_result);
        }

        Ok(results)
    }
}