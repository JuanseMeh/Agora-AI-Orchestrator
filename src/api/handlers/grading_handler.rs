use tonic::{Request, Response, Status};
use std::sync::Arc;
use crate::api::proto::ai_service_server::AiService;
use crate::api::proto::{
    ApproveSuggestionRequest, ApproveSuggestionResponse,
    GradeAssignmentRequest, GradeAssignmentResponse,
    SuggestAssignmentRequest, SuggestAssignmentResponse, SuggestionStats,
    GradingResult as ProtoGradingResult,
    CriterionResult as ProtoCriterionResult,
};
use crate::context::aggregator::{ContextAggregator, AggregatorError};
use crate::context::suggestion_cache::{SuggestionCache, SuggestionCacheEntry, SuggestionCacheError};
use crate::context::user_config_client::{UserConfigClient, UserConfigClientError};
use crate::context::workspace_client::{WorkspaceClient, WorkspaceClientError, GradeWritePayload, CriterionResultPayload};
use crate::domain::ports::llm_provider::LlmError;
use crate::orchestration::orchestrator::{Orchestrator, OrchestratorError};
use crate::orchestration::intent_router::OrchestratorRequest;
use crate::models::grading::grading_result::GradingResult;
use chrono::Utc;
use uuid::Uuid;

pub struct GradingHandler {
    orchestrator: Arc<Orchestrator>,
    aggregator: Arc<ContextAggregator>,
    user_config_client: Arc<UserConfigClient>,
    suggestion_cache: Arc<SuggestionCache>,
    workspace_client: Arc<WorkspaceClient>,
}

impl GradingHandler {
    pub fn new(
        orchestrator: Arc<Orchestrator>,
        aggregator: Arc<ContextAggregator>,
        user_config_client: Arc<UserConfigClient>,
        suggestion_cache: Arc<SuggestionCache>,
        workspace_client: Arc<WorkspaceClient>,
    ) -> Self {
        Self { orchestrator, aggregator, user_config_client, suggestion_cache, workspace_client }
    }
}

#[tonic::async_trait]
impl AiService for GradingHandler {
    async fn suggest_assignment(
        &self,
        request: Request<SuggestAssignmentRequest>,
    ) -> Result<Response<SuggestAssignmentResponse>, Status> {
        let req = request.into_inner();
        let requester_id = Uuid::parse_str(&req.requester_user_id)
            .map_err(|_| Status::invalid_argument("requester_user_id must be a valid UUID"))?;

        let profile = self.user_config_client
            .fetch_ai_profile(requester_id)
            .await
            .map_err(map_user_config_error)?;

        if !profile.agentic_mode {
            return Err(Status::failed_precondition("agentic mode is disabled for this user"));
        }

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

        let suggestion_id = Uuid::new_v4();
        self.suggestion_cache.put(SuggestionCacheEntry {
                suggestion_id,
                workspace_id: req.workspace_id,
                assignment_id: req.assignment_id,
                requester_user_id: req.requester_user_id.clone(),
                created_at: Utc::now(),
                results: results.clone(),
            })
            .await
            .map_err(map_suggestion_cache_error)?;

        let stats = Some(to_stats(&results));
        let proto_results = results.into_iter().map(into_proto_result).collect();

        Ok(Response::new(SuggestAssignmentResponse {
            suggestion_id: suggestion_id.to_string(),
            results: proto_results,
            stats,
        }))
    }

    async fn approve_suggestion(
        &self,
        request: Request<ApproveSuggestionRequest>,
    ) -> Result<Response<ApproveSuggestionResponse>, Status> {
        let req = request.into_inner();
        let suggestion_id = Uuid::parse_str(&req.suggestion_id)
            .map_err(|_| Status::invalid_argument("suggestion_id must be a valid UUID"))?;

        let cached = self.suggestion_cache
            .get(&suggestion_id)
            .await
            .map_err(map_suggestion_cache_error)?
            .ok_or_else(|| Status::not_found("suggestion not found in cache"))?;

        self.suggestion_cache
            .remove(&suggestion_id)
            .await
            .map_err(map_suggestion_cache_error)?;

        persist_results(&self.workspace_client, &cached.results)
            .await
            .map_err(map_workspace_client_error)?;

        Ok(Response::new(ApproveSuggestionResponse {
            suggestion_id: req.suggestion_id,
            results: cached.results.into_iter().map(into_proto_result).collect(),
        }))
    }

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

        persist_results(&self.workspace_client, &results)
            .await
            .map_err(map_workspace_client_error)?;

        let proto_results = results.into_iter().map(into_proto_result).collect();

        Ok(Response::new(GradeAssignmentResponse {
            results: proto_results,
        }))
    }
}

fn to_stats(results: &[GradingResult]) -> SuggestionStats {
    let graded_count = results.len() as i32;
    let max_score = results.first().map(|r| r.max_score).unwrap_or(0.0);
    let average_score = if results.is_empty() {
        0.0
    } else {
        results.iter().map(|r| r.total_score).sum::<f64>() / results.len() as f64
    };

    SuggestionStats {
        average_score,
        max_score,
        graded_submissions: graded_count,
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

async fn persist_results(
    workspace_client: &WorkspaceClient,
    results: &[GradingResult],
) -> Result<(), WorkspaceClientError> {
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

        workspace_client.write_grade(result.submission_id, payload).await?;
    }

    Ok(())
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
        OrchestratorError::GradingFailed(LlmError::ProviderError { status: 429, message }) => {
            Status::resource_exhausted(message)
        }
        OrchestratorError::GradingFailed(err) => Status::internal(err.to_string()),
        OrchestratorError::NotImplemented(msg) => Status::unimplemented(msg),
    }
}

pub fn map_user_config_error(e: UserConfigClientError) -> Status {
    Status::internal(format!("user profile fetch failed: {}", e))
}

pub fn map_suggestion_cache_error(e: SuggestionCacheError) -> Status {
    Status::internal(format!("suggestion cache error: {}", e))
}

pub fn map_workspace_client_error(e: WorkspaceClientError) -> Status {
    Status::internal(format!("workspace persistence error: {}", e))
}
