#![allow(dead_code)]


use tonic::{Request, Response, Status};
use std::sync::Arc;
use crate::api::proto::ai_service_server::AiService;
use crate::api::proto::{GradeAssignmentRequest, GradeAssignmentResponse};
use crate::orchestration::orchestrator::{Orchestrator, OrchestratorError};
use crate::orchestration::intent_router::OrchestratorRequest;
/// gRPC handler for the GradeAssignment RPC.
///
/// Receives the raw proto request, constructs the orchestrator request,
/// delegates to the `Orchestrator`, and maps the result back to a proto response.
/// This is the only file in the codebase that knows about both proto types
/// and orchestrator types simultaneously.
pub struct GradingHandler {
    orchestrator: Arc<Orchestrator>,
}

impl GradingHandler {
    pub fn new(orchestrator: Arc<Orchestrator>) -> Self {
        Self { orchestrator }
    }
}

#[tonic::async_trait]
impl AiService for GradingHandler {
    async fn grade_assignment(
        &self,
        request: Request<GradeAssignmentRequest>,
    ) -> Result<Response<GradeAssignmentResponse>, Status> {
        let req = request.into_inner();

        let orchestrator_request = OrchestratorRequest::GradeAssignment {
            workspace_id: req.workspace_id,
            assignment_id: req.assignment_id,
        };

        // TODO: context aggregation layer will replace these placeholders
        // once the workspace client is built. For now the orchestrator
        // cannot be called without real assignment + grading context.
        // This will be wired in the next layer.
        let _ = orchestrator_request;

        Err(Status::unimplemented(
            "context aggregation not yet wired — coming in next layer",
        ))
    }
}

/// Maps an `OrchestratorError` to a gRPC `Status`.
/// Kept as a standalone function so it can be tested independently.
pub fn map_orchestrator_error(error: OrchestratorError) -> Status {
    match error {
        OrchestratorError::GradingFailed(msg) => {
            Status::internal(format!("grading failed: {}", msg))
        }
        OrchestratorError::NotImplemented(msg) => {
            Status::unimplemented(format!("not implemented: {}", msg))
        }
    }
} 