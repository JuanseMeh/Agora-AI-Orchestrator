use tonic::{Request, Response, Status};
use std::sync::Arc;
use crate::api::proto::ai_service_server::AiService;
use crate::api::proto::{
    GradeAssignmentRequest, GradeAssignmentResponse,
    GradingResult as ProtoGradingResult,
    CriterionResult as ProtoCriterionResult,
};
use crate::context::aggregator::{ContextAggregator, AggregatorError};
use crate::orchestration::orchestrator::{Orchestrator, OrchestratorError};
use crate::orchestration::intent_router::OrchestratorRequest;
use crate::models::grading::grading_result::GradingResult;

pub struct GradingHandler {
    orchestrator: Arc<Orchestrator>,
    aggregator: Arc<ContextAggregator>,
}

impl GradingHandler {
    pub fn new(orchestrator: Arc<Orchestrator>, aggregator: Arc<ContextAggregator>) -> Self {
        Self { orchestrator, aggregator }
    }
}

#[tonic::async_trait]
impl AiService for GradingHandler {
    async fn grade_assignment(
        &self,
        request: Request<GradeAssignmentRequest>,
    ) -> Result<Response<GradeAssignmentResponse>, Status> {
        let req = request.into_inner();

        let (assignment, context) = self.aggregator
            .build_grading_context(req.assignment_id)
            .await
            .map_err(map_aggregator_error)?;

        let orchestrator_request = OrchestratorRequest::GradeAssignment {
            workspace_id: req.workspace_id,
            assignment_id: req.assignment_id,
        };

        let results = self.orchestrator
            .dispatch(orchestrator_request, &assignment, &context)
            .await
            .map_err(map_orchestrator_error)?;

        let proto_results = results.into_iter().map(into_proto_result).collect();

        Ok(Response::new(GradeAssignmentResponse {
            results: proto_results,
        }))
    }
}

fn into_proto_result(r: GradingResult) -> ProtoGradingResult {
    ProtoGradingResult {
        result_id: r.result_id.to_string(),
        submission_id: r.submission_id,
        total_score: r.total_score,
        max_score: r.max_score,
        feedback_summary: r.feedback_summary,
        grading_model: r.grading_model,
        evaluated_at: r.evaluated_at.to_rfc3339(),
        criteria_results: r.criteria_results.into_iter().map(|c| {
            ProtoCriterionResult {
                criterion_id: c.criterion_id,
                criterion_name: c.criterion_name,
                score: c.score,
                max_score: c.max_score,
                feedback: c.feedback,
                matched_level: c.matched_level,
            }
        }).collect(),
    }
}

pub fn map_aggregator_error(e: AggregatorError) -> Status {
    match e {
        AggregatorError::AssignmentNotClosed { id } => {
            Status::failed_precondition(format!("assignment {} is not closed", id))
        }
        AggregatorError::NoSubmissions { assignment_id } => {
            Status::not_found(format!("no submissions for assignment {}", assignment_id))
        }
        AggregatorError::WorkspaceClient(e) => {
            Status::internal(e.to_string())
        }
        AggregatorError::RubricParseFailed { assignment_id, reason } => {
            Status::internal(format!("rubric parse failed for {}: {}", assignment_id, reason))
        }
        AggregatorError::ContentExtractionFailed { submission_id } => {
            Status::internal(format!("content extraction failed for submission {}", submission_id))
        }
    }
}

pub fn map_orchestrator_error(e: OrchestratorError) -> Status {
    match e {
        OrchestratorError::GradingFailed(msg) => Status::internal(msg),
        OrchestratorError::NotImplemented(msg) => Status::unimplemented(msg),
    }
}