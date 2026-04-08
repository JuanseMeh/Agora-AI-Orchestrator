use crate::models::execution::execution_plan::WorkflowType;

/// Represents an incoming request to the AI module.
///
/// Constructed from API handler input or event payloads.
/// The router maps this to a `WorkflowType` which the orchestrator
/// uses to select the correct planner.
#[derive(Debug, Clone)]
pub enum OrchestratorRequest {
    GradeAssignment {
        workspace_id: i32,
        assignment_id: i32,
    },
    GenerateClassReport {
        workspace_id: i32,
        assignment_id: i32,
    },
    GenerateTeachingSuggestions {
        workspace_id: i32,
        course_id: i32,
    },
}

/// Routes an `OrchestratorRequest` to the appropriate `WorkflowType`.
///
/// This is a pure mapping layer — no logic, no IO.
/// Adding a new workflow means adding a variant here and a planner below.
pub struct IntentRouter;

impl IntentRouter {
    pub fn route(request: &OrchestratorRequest) -> WorkflowType {
        match request {
            OrchestratorRequest::GradeAssignment { .. } => {
                WorkflowType::GradingWorkflow
            }
            OrchestratorRequest::GenerateClassReport { .. } => {
                WorkflowType::AnalyticsWorkflow
            }
            OrchestratorRequest::GenerateTeachingSuggestions { .. } => {
                WorkflowType::RecommendationWorkflow
            }
        }
    }
}