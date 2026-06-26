use std::sync::Arc;

use thiserror::Error;

use crate::application::workflows::grading::grading_workflow::{GradingWorkflow, WorkflowContext, WorkflowError};
use crate::context::aggregator::SubmissionFilter;
use crate::domain::ports::llm_provider::LlmError;
use crate::models::grading::grading_result::GradingResult;
use crate::orchestration::intent_router::{IntentRouter, OrchestratorRequest};
use crate::orchestration::planners::grading_planners::GradingPlanner;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("workflow failed: {0}")]
    WorkflowFailed(#[from] WorkflowError),

    #[error("workflow not implemented for MVP: {0}")]
    NotImplemented(String),
}

impl OrchestratorError {
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, OrchestratorError::WorkflowFailed(
            WorkflowError::Llm(LlmError::ProviderError { status: 429, .. })
        ))
    }
}

/// Top-level coordinator for the AI module.
///
/// Creates execution plans via planners and runs them through the
/// appropriate workflow. This is the single entry point the API layer
/// calls — it never touches workflows or planners directly.
pub struct Orchestrator {
    ctx: Arc<WorkflowContext>,
}

impl Orchestrator {
    pub fn new(ctx: Arc<WorkflowContext>) -> Self {
        Self { ctx }
    }

    /// Dispatches a request through the plan → workflow pipeline.
    ///
    /// Builds an ExecutionPlan via the appropriate planner, then runs
    /// it through the matched workflow. The `persist` flag controls
    /// whether the plan includes the `save_grades` step.
    /// `retro_style` and `exigency_level` come from the teacher's AI profile
    /// and are injected into the LLM prompt to control feedback style and strictness.
    pub async fn dispatch(
        &self,
        request: &OrchestratorRequest,
        filter: &SubmissionFilter,
        persist: bool,
        retro_style: &str,
        exigency_level: &str,
    ) -> Result<Vec<GradingResult>, OrchestratorError> {
        let (workspace_id, assignment_id) = match request {
            OrchestratorRequest::GradeAssignment { workspace_id, assignment_id } => {
                (*workspace_id, *assignment_id)
            }
            _ => return Err(OrchestratorError::NotImplemented(
                "only GradeAssignment is implemented".to_string()
            )),
        };

        let workflow_type = IntentRouter::route(request);
        match workflow_type {
            crate::models::execution::execution_plan::WorkflowType::GradingWorkflow => {
                let mut plan = GradingPlanner::build(workspace_id, assignment_id, persist);
                let workflow = GradingWorkflow::new(self.ctx.clone());
                workflow.run(&mut plan, assignment_id, filter, retro_style, exigency_level)
                    .await
                    .map_err(OrchestratorError::from)
            }
            other => Err(OrchestratorError::NotImplemented(other.to_string())),
        }
    }
}
