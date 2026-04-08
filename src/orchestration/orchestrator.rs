// orchestration/orchestrator.rs

use crate::application::workflows::grading::grading_workflow::GradingWorkflow;
use crate::domain::ports::llm_provider::LlmProvider;
use crate::models::context::assignment_context::AssignmentContext;
use crate::models::context::submission_context::GradingContext;
use crate::models::grading::grading_result::GradingResult;
use crate::orchestration::intent_router::{IntentRouter, OrchestratorRequest};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("grading failed: {0}")]
    GradingFailed(String),

    #[error("workflow not implemented for MVP: {0}")]
    NotImplemented(String),
}

/// Top-level coordinator for the AI module.
///
/// Receives an `OrchestratorRequest`, routes it to the correct workflow,
/// and returns the result. This is the single entry point the API layer
/// calls — it never touches workflows or planners directly.
pub struct Orchestrator {
    provider: Arc<dyn LlmProvider>,
}

impl Orchestrator {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Dispatches a request to the appropriate workflow.
    pub async fn dispatch(
        &self,
        request: OrchestratorRequest,
        assignment: &AssignmentContext,
        context: &GradingContext,
    ) -> Result<Vec<GradingResult>, OrchestratorError> {
        let workflow_type = IntentRouter::route(&request);

        match workflow_type {
            crate::models::execution::execution_plan::WorkflowType::GradingWorkflow => {
                self.run_grading(assignment, context).await
            }
            other => Err(OrchestratorError::NotImplemented(other.to_string())),
        }
    }

    async fn run_grading(
        &self,
        assignment: &AssignmentContext,
        context: &GradingContext,
    ) -> Result<Vec<GradingResult>, OrchestratorError> {
        let workflow = GradingWorkflow::new(self.provider.as_ref());

        workflow
            .run(assignment, context)
            .await
            .map_err(|e| OrchestratorError::GradingFailed(e.to_string()))
    }
}