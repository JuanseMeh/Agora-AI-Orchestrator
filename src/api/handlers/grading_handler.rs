use tonic::{Request, Response, Status};
use std::sync::Arc;
use crate::api::proto::ai_service_server::AiService;
use crate::api::proto::{
    ApproveSuggestionRequest, ApproveSuggestionResponse,
    GradeAssignmentRequest, GradeAssignmentResponse,
    GeneratePerformanceReportRequest, GeneratePerformanceReportResponse,
    AssignmentPerformance as ProtoAssignmentPerformance,
    StudentPerformance as ProtoStudentPerformance,
    SuggestAssignmentRequest, SuggestAssignmentResponse, SuggestionStats,
    GradingResult as ProtoGradingResult,
    CriterionResult as ProtoCriterionResult,
    CriterionOverride,
};
use crate::application::prompts::performance_report_prompt::PerformanceReportPromptBuilder;
use crate::context::aggregator::{ContextAggregator, SubmissionFilter};
use crate::context::llm_cache::{LlmCache, LlmCacheHandle};
use crate::context::suggestion_cache::{SuggestionCache, SuggestionCacheEntry, SuggestionCacheError};
use crate::context::user_config_client::{UserConfigClient, UserConfigClientError};
use crate::context::vector_store::VectorStoreHandle;
use crate::context::workspace_client::{WorkspaceClient, WorkspaceClientError, GradeWritePayload, CriterionResultPayload};
use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::orchestration::orchestrator::{Orchestrator, OrchestratorError};
use crate::orchestration::intent_router::OrchestratorRequest;
use crate::models::grading::grading_result::GradingResult;
use chrono::Utc;
use uuid::Uuid;

pub struct GradingHandler {
    orchestrator: Arc<Orchestrator>,
    user_config_client: Arc<UserConfigClient>,
    suggestion_cache: Arc<SuggestionCache>,
    workspace_client: Arc<WorkspaceClient>,
    provider: Arc<dyn LlmProvider>,
    vector_store: Option<VectorStoreHandle>,
    llm_cache: Option<LlmCacheHandle>,
}

impl GradingHandler {
    pub fn new(
        orchestrator: Arc<Orchestrator>,
        user_config_client: Arc<UserConfigClient>,
        suggestion_cache: Arc<SuggestionCache>,
        workspace_client: Arc<WorkspaceClient>,
        provider: Arc<dyn LlmProvider>,
        vector_store: Option<VectorStoreHandle>,
        llm_cache: Option<LlmCacheHandle>,
    ) -> Self {
        Self {
            orchestrator,
            user_config_client,
            suggestion_cache,
            workspace_client,
            provider,
            vector_store,
            llm_cache,
        }
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

        let user_ids = ContextAggregator::parse_user_filters(&req.user_ids)
            .map_err(|e| Status::invalid_argument(format!("invalid user_id in filter: {}", e)))?;
        let filter = SubmissionFilter {
            submission_ids: req.submission_ids.clone(),
            user_ids,
            include_already_graded: req.include_already_graded,
        };

        let orchestrator_request = OrchestratorRequest::GradeAssignment {
            workspace_id: req.workspace_id,
            assignment_id: req.assignment_id,
        };

        let results = self.orchestrator
            .dispatch(&orchestrator_request, &filter, false, &profile.retro_style, &profile.exigency_level)
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

        let mut cached = self.suggestion_cache
            .get(&suggestion_id)
            .await
            .map_err(map_suggestion_cache_error)?
            .ok_or_else(|| Status::not_found("suggestion not found in cache"))?;

        self.suggestion_cache
            .remove(&suggestion_id)
            .await
            .map_err(map_suggestion_cache_error)?;

        // Apply teacher overrides before persisting
        if !req.overrides.is_empty() {
            self.apply_overrides(&mut cached.results, &req.overrides).await;
        }

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
        let user_ids = ContextAggregator::parse_user_filters(&req.user_ids)
            .map_err(|e| Status::invalid_argument(format!("invalid user_id in filter: {}", e)))?;
        let filter = SubmissionFilter {
            submission_ids: req.submission_ids.clone(),
            user_ids,
            include_already_graded: req.include_already_graded,
        };

        let orchestrator_request = OrchestratorRequest::GradeAssignment {
            workspace_id: req.workspace_id,
            assignment_id: req.assignment_id,
        };

        let results = self.orchestrator
            .dispatch(&orchestrator_request, &filter, true, "detailed", "moderated")
            .await
            .map_err(map_orchestrator_error)?;

        let proto_results = results.into_iter().map(into_proto_result).collect();

        Ok(Response::new(GradeAssignmentResponse {
            results: proto_results,
        }))
    }

    async fn generate_performance_report(
        &self,
        request: Request<GeneratePerformanceReportRequest>,
    ) -> Result<Response<GeneratePerformanceReportResponse>, Status> {
        let req = request.into_inner();

        let dataset = self.workspace_client
            .fetch_performance_data(req.workspace_id, req.assignment_id)
            .await
            .map_err(map_workspace_client_error)?;

        let dataset_json = serde_json::to_string(&serde_json::json!({
            "workspaceId": dataset.workspace_id,
            "assignmentId": dataset.assignment_id,
            "summary": {
                "totalAssignments": dataset.summary.total_assignments,
                "totalSubmissions": dataset.summary.total_submissions,
                "gradedSubmissions": dataset.summary.graded_submissions,
                "pendingSubmissions": dataset.summary.pending_submissions,
                "averageScore": dataset.summary.average_score,
            },
            "assignments": dataset.assignments.iter().map(|a| serde_json::json!({
                "assignmentId": a.assignment_id,
                "assignmentName": a.assignment_name,
                "totalSubmissions": a.total_submissions,
                "gradedSubmissions": a.graded_submissions,
                "pendingSubmissions": a.pending_submissions,
                "averageScore": a.average_score,
                "maxScore": a.max_score,
                "failedSubmissions": a.failed_submissions,
                "failureRate": a.failure_rate,
            })).collect::<Vec<_>>(),
            "students": dataset.students.iter().map(|s| serde_json::json!({
                "userId": s.user_id,
                "totalSubmissions": s.total_submissions,
                "gradedSubmissions": s.graded_submissions,
                "pendingSubmissions": s.pending_submissions,
                "averageScore": s.average_score,
            })).collect::<Vec<_>>(),
        }))
            .map_err(|e| Status::internal(format!("failed to serialize dataset: {}", e)))?;

        let prompt = PerformanceReportPromptBuilder::build(
            req.workspace_id,
            req.assignment_id,
            &dataset_json,
        );

        let ai_analysis = self.provider
            .generate_text(prompt)
            .await
            .map_err(map_llm_error)?;

        let response = GeneratePerformanceReportResponse {
            workspace_id: dataset.workspace_id,
            assignment_id: dataset.assignment_id,
            total_assignments: dataset.summary.total_assignments,
            total_submissions: dataset.summary.total_submissions,
            graded_submissions: dataset.summary.graded_submissions,
            pending_submissions: dataset.summary.pending_submissions,
            average_score: dataset.summary.average_score,
            ai_analysis,
            assignments: dataset.assignments.into_iter().map(|assignment| ProtoAssignmentPerformance {
                assignment_id: assignment.assignment_id,
                assignment_name: assignment.assignment_name,
                total_submissions: assignment.total_submissions,
                graded_submissions: assignment.graded_submissions,
                pending_submissions: assignment.pending_submissions,
                average_score: assignment.average_score,
                max_score: assignment.max_score,
                failed_submissions: assignment.failed_submissions,
                failure_rate: assignment.failure_rate,
            }).collect(),
            students: dataset.students.into_iter().map(|student| ProtoStudentPerformance {
                user_id: student.user_id.to_string(),
                total_submissions: student.total_submissions,
                graded_submissions: student.graded_submissions,
                pending_submissions: student.pending_submissions,
                average_score: student.average_score,
            }).collect(),
        };

        Ok(Response::new(response))
    }
}

impl GradingHandler {
    /// Applies teacher overrides to cached grading results.
    ///
    /// For each override:
    /// 1. Updates the matching submission's criterion score/feedback
    /// 2. Recalculates the submission total score
    /// 3. Stores the corrected example in Qdrant with `teacher_corrected: true`
    /// 4. Invalidates the Redis LLM cache for that (submission_id, criterion_id)
    async fn apply_overrides(
        &self,
        results: &mut Vec<GradingResult>,
        overrides: &[CriterionOverride],
    ) {
        for override_ in overrides {
            let submission_id = override_.submission_id;
            let criterion_id = &override_.criterion_id;

            // Find and update the matching result + criterion
            let mut found = false;
            for result in results.iter_mut() {
                if result.submission_id != submission_id {
                    continue;
                }
                for criterion in result.criteria_results.iter_mut() {
                    if criterion.criterion_id != *criterion_id {
                        continue;
                    }
                    criterion.score = override_.teacher_score;
                    if !override_.teacher_feedback.is_empty() {
                        criterion.feedback = override_.teacher_feedback.clone();
                    }
                    result.total_score = result.criteria_results.iter().map(|c| c.score).sum();
                    found = true;
                    break;
                }
                if found {
                    break;
                }
            }

            if !found {
                tracing::warn!(
                    submission_id = submission_id,
                    criterion_id = criterion_id,
                    "teacher override target not found in cached results"
                );
                continue;
            }

            // Store the corrected example in Qdrant
            if let Some(ref store) = self.vector_store {
                let point_id = Uuid::new_v4().to_string();
                let embed_text = format!(
                    "Criterion: {}\nScore: {:.1}/{:.1}\nFeedback: {}",
                    criterion_id,
                    override_.teacher_score,
                    0.0, // max_score not available from proto; stored as 0
                    override_.teacher_feedback,
                );

                match self.provider.embed_text(embed_text).await {
                    Ok(vector) => {
                        if let Err(e) = store.store_teacher_correction(
                            &point_id,
                            submission_id,
                            0, // assignment_id unknown at this point; corrected entries use 0
                            0, // workspace_id unknown
                            criterion_id,
                            "",
                            override_.teacher_score,
                            0.0,
                            &override_.teacher_feedback,
                            "",
                            vector,
                        ).await {
                            tracing::warn!(
                                error = %e,
                                submission_id = submission_id,
                                "failed to store teacher correction in Qdrant"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            submission_id = submission_id,
                            "failed to embed teacher correction"
                        );
                    }
                }
            }

            // Invalidate the LLM cache for this (submission_id, criterion_id)
            if let Some(ref cache) = self.llm_cache {
                let cache_key = LlmCache::build_key(submission_id, criterion_id);
                if let Err(e) = cache.invalidate(&cache_key).await {
                    tracing::warn!(
                        error = %e,
                        cache_key = %cache_key,
                        "failed to invalidate LLM cache after teacher override"
                    );
                }
            }
        }
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

pub fn map_orchestrator_error(e: OrchestratorError) -> Status {
    match e {
        OrchestratorError::NotImplemented(msg) => Status::unimplemented(msg),
        _ if e.is_rate_limited() => Status::resource_exhausted(e.to_string()),
        OrchestratorError::WorkflowFailed(inner) => {
            Status::internal(inner.to_string())
        }
    }
}

pub fn map_user_config_error(e: UserConfigClientError) -> Status {
    Status::internal(format!("user profile fetch failed: {}", e))
}

pub fn map_suggestion_cache_error(e: SuggestionCacheError) -> Status {
    Status::internal(format!("suggestion cache error: {}", e))
}

pub fn map_workspace_client_error(e: WorkspaceClientError) -> Status {
    Status::internal(format!("workspace service error: {}", e))
}

pub fn map_llm_error(e: LlmError) -> Status {
    match e {
        LlmError::ProviderError { status: 429, message } => Status::resource_exhausted(message),
        other => Status::internal(other.to_string()),
    }
}
