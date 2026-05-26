use std::sync::Arc;

use futures::future::try_join_all;
use tokio::sync::Semaphore;

use crate::application::prompts::criterion_prompt::CriterionPromptBuilder;
use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::models::grading::rubric::RubricCriterion;
use crate::models::grading::grading_result::CriterionResult;
use crate::models::context::submission_context::SubmissionContext;

/// Evaluates all rubric criteria for a single submission.
///
/// Evaluates every criterion against the submission in parallel,
/// with concurrency gated by a shared semaphore to avoid overloading
/// the LLM provider. Criterion order in the output matches input order.
pub struct EvaluateCriteria<'a> {
    provider: &'a dyn LlmProvider,
    semaphore: Arc<Semaphore>,
}

impl<'a> EvaluateCriteria<'a> {
    pub fn new(provider: &'a dyn LlmProvider, semaphore: Arc<Semaphore>) -> Self {
        Self { provider, semaphore }
    }

    /// Evaluates every criterion in `criteria` against the submission content.
    /// All criteria run concurrently, bounded by the shared semaphore.
    /// Returns a `Vec<CriterionResult>` in the same order as the input criteria.
    pub async fn run(
        &self,
        criteria: &[RubricCriterion],
        submission: &SubmissionContext,
    ) -> Result<Vec<CriterionResult>, LlmError> {
        let content = &submission.content;

        let futs: Vec<_> = criteria.iter().map(|criterion| {
            let prompt = CriterionPromptBuilder::build(criterion, content);
            async {
                let _permit = self.semaphore.acquire().await.expect("semaphore not closed");
                self.provider.evaluate_criterion(prompt).await
            }
        }).collect();

        try_join_all(futs).await
    }
}
