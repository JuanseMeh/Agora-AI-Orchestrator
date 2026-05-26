use std::sync::Arc;

use futures::future::try_join_all;
use thiserror::Error;
use tokio::sync::Semaphore;

use crate::application::workflows::grading::steps::aggregate_scores::AggregateScores;
use crate::application::workflows::grading::steps::evaluate_criteria::EvaluateCriteria;
use crate::context::aggregator::{AggregatorError, ContextAggregator, SubmissionFilter};
use crate::context::workspace_client::{WorkspaceClient, WorkspaceClientError, GradeWritePayload, CriterionResultPayload};
use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::models::context::assignment_context::AssignmentContext;
use crate::models::context::submission_context::GradingContext;
use crate::models::execution::execution_plan::ExecutionPlan;
use crate::models::grading::grading_result::{CriterionResult, GradingResult};

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("aggregator error: {0}")]
    Aggregator(#[from] AggregatorError),

    #[error("llm error: {0}")]
    Llm(#[from] LlmError),

    #[error("workspace client error: {0}")]
    WorkspaceClient(#[from] WorkspaceClientError),

    #[error("unknown step in plan: {0}")]
    UnknownStep(String),

    #[error("{0}")]
    Internal(String),
}

/// All services the grading workflow needs to execute its plan.
pub struct WorkflowContext {
    pub provider: Arc<dyn LlmProvider>,
    pub aggregator: Arc<ContextAggregator>,
    pub workspace_client: Arc<WorkspaceClient>,
}

const DEFAULT_MAX_CONCURRENT: usize = 5;

fn max_concurrent_llm_calls() -> usize {
    std::env::var("MAX_CONCURRENT_LLM_CALLS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_CONCURRENT)
}

const DEFAULT_GRADING_MODEL: &str = "gemini-2.0-flash";

fn grading_model() -> String {
    std::env::var("GEMINI_MODEL")
        .ok()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| DEFAULT_GRADING_MODEL.to_string())
}

/// Orchestrates the full grading pipeline by walking an ExecutionPlan.
///
/// Iterates the plan steps in order, dispatching each to the appropriate
/// handler. Tracks step status (Running → Completed/Failed) for observability.
/// Submissions are processed in parallel; criteria within a submission
/// are also parallel, gated by a shared concurrency semaphore.
pub struct GradingWorkflow {
    ctx: Arc<WorkflowContext>,
}

impl GradingWorkflow {
    pub fn new(ctx: Arc<WorkflowContext>) -> Self {
        Self { ctx }
    }

    /// Executes the plan for a given assignment and submission filter.
    pub async fn run(
        &self,
        plan: &mut ExecutionPlan,
        assignment_id: i32,
        filter: &SubmissionFilter,
    ) -> Result<Vec<GradingResult>, WorkflowError> {
        plan.steps.sort_by_key(|s| s.order);

        let semaphore = Arc::new(Semaphore::new(max_concurrent_llm_calls()));

        let mut assignment: Option<AssignmentContext> = None;
        let mut context: Option<GradingContext> = None;
        let mut results: Option<Vec<GradingResult>> = None;

        for step in &mut plan.steps {
            step.mark_running();

            let tool_name = step.tool_name.clone();
            match tool_name.as_str() {
                "retrieve_assignment" | "retrieve_submissions" => {
                    if assignment.is_none() {
                        let (a, c) = self.ctx
                            .aggregator
                            .build_grading_context_with_filter(assignment_id, filter)
                            .await?;
                        assignment = Some(a);
                        context = Some(c);
                    }
                    step.mark_completed(serde_json::json!({"status": "ok"}));
                }

                "evaluate_criteria" => {
                    let assignment = assignment.as_ref()
                        .ok_or_else(|| WorkflowError::Internal("missing assignment context".into()))?;
                    let context = context.as_ref()
                        .ok_or_else(|| WorkflowError::Internal("missing grading context".into()))?;

                    let evaluator = EvaluateCriteria::new(self.ctx.provider.as_ref(), semaphore.clone());

                    // Process all submissions in parallel, each evaluating criteria concurrently
                    let grading_tasks: Vec<_> = context.submissions.iter().map(|submission| {
                        let criteria = &assignment.rubric.criteria;
                        async {
                            let criteria_results = evaluator.run(criteria, submission).await?;
                            Ok::<_, WorkflowError>(criteria_results)
                        }
                    }).collect();

                    let all_results: Vec<Vec<CriterionResult>> =
                        try_join_all(grading_tasks).await?;

                    let model = grading_model();
                    results = Some(
                        context.submissions.iter()
                            .zip(all_results.into_iter())
                            .map(|(submission, criteria_results)| {
                                AggregateScores::run(submission, &assignment.rubric, criteria_results, &model)
                            })
                            .collect()
                    );

                    let count = results.as_ref().map(|r| r.len()).unwrap_or(0);
                    step.mark_completed(serde_json::json!({"count": count}));
                }

                "aggregate_scores" => {
                    // Already folded into evaluate_criteria above — nothing to do.
                    step.mark_completed(serde_json::json!({"status": "skipped"}));
                }

                "save_grades" => {
                    let results = results.as_ref()
                        .ok_or_else(|| WorkflowError::Internal("no results to persist".into()))?;

                    for result in results {
                        let payload = GradeWritePayload {
                            score: result.total_score,
                            feedback: result.feedback_summary.clone(),
                            rubric_results: result.criteria_results.iter().map(|c| CriterionResultPayload {
                                criterion_id: c.criterion_id.clone(),
                                score: c.score,
                                feedback: c.feedback.clone(),
                            }).collect(),
                            evaluated_at: result.evaluated_at.to_rfc3339(),
                        };
                        self.ctx.workspace_client.write_grade(result.submission_id, payload).await?;
                    }

                    step.mark_completed(serde_json::json!({"count": results.len()}));
                }

                other => {
                    step.mark_failed(format!("unknown step: {}", other));
                    return Err(WorkflowError::UnknownStep(other.to_string()));
                }
            }
        }

        results.ok_or_else(|| WorkflowError::Internal("plan produced no results".into()))
    }
}
