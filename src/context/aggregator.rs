// context/aggregator.rs

use crate::context::workspace_client::{
    WorkspaceClient, WorkspaceClientError, AssignmentResponse, SubmissionResponse,
};
use crate::models::context::assignment_context::AssignmentContext;
use crate::models::context::submission_context::{GradingContext, SubmissionContext};
use crate::models::grading::rubric::Rubric;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AggregatorError {
    #[error("workspace client error: {0}")]
    WorkspaceClient(#[from] WorkspaceClientError),

    #[error("assignment {id} is not closed — grading only runs on closed assignments")]
    AssignmentNotClosed { id: i32 },

    #[error("no submissions found for assignment {assignment_id}")]
    NoSubmissions { assignment_id: i32 },

    #[error("failed to parse rubric for assignment {assignment_id}: {reason}")]
    RubricParseFailed { assignment_id: i32, reason: String },

    #[error("failed to extract submission content for submission {submission_id}")]
    ContentExtractionFailed { submission_id: i32 },
}

/// Assembles a `GradingContext` from raw Workspace Service data.
///
/// This is the only place in the system that knows how to map
/// Workspace Service response shapes onto internal domain models.
pub struct ContextAggregator {
    client: WorkspaceClient,
}

impl ContextAggregator {
    pub fn new(client: WorkspaceClient) -> Self {
        Self { client }
    }

    pub fn from_env() -> Result<Self, WorkspaceClientError> {
        Ok(Self {
            client: WorkspaceClient::from_env()?,
        })
    }

    /// Builds a complete `GradingContext` for the given assignment.
    ///
    /// Validates that the assignment is closed (status = 2) before
    /// proceeding — grading is only valid on closed assignments.
    pub async fn build_grading_context(
        &self,
        assignment_id: i32,
    ) -> Result<(AssignmentContext, GradingContext), AggregatorError> {
        let raw_assignment = self.client.fetch_assignment(assignment_id).await?;

        // status 2 = Cerrado — only closed assignments can be graded
        if raw_assignment.status != 2 {
            return Err(AggregatorError::AssignmentNotClosed { id: assignment_id });
        }

        let raw_submissions = self.client.fetch_submissions(assignment_id).await?;

        if raw_submissions.is_empty() {
            return Err(AggregatorError::NoSubmissions { assignment_id });
        }

        let assignment_context = self.map_assignment(&raw_assignment)?;
        let workspace_id = raw_assignment.workspace_id;

        let submission_contexts = raw_submissions
            .iter()
            .map(|s| self.map_submission(s))
            .collect::<Result<Vec<_>, _>>()?;

        let grading_context = GradingContext::new(
            assignment_id,
            workspace_id,
            submission_contexts,
        );

        Ok((assignment_context, grading_context))
    }

    /// Maps a raw `AssignmentResponse` onto `AssignmentContext`.
    fn map_assignment(
        &self,
        raw: &AssignmentResponse,
    ) -> Result<AssignmentContext, AggregatorError> {
        let rubric = self.parse_rubric(raw.id, &raw.rubric)?;

        Ok(AssignmentContext {
            assignment_id: raw.id,
            workspace_id: raw.workspace_id,
            course_id: raw.workspace_id, // workspace is the course scope for now
            title: raw.name.clone(),
            description: raw.description.clone(),
            rubric,
            learning_objectives: None,
            created_at: raw.due_date,
        })
    }

    /// Maps a raw `SubmissionResponse` onto `SubmissionContext`.
    ///
    /// Extracts the submission text from the `content` JSONB field.
    /// Looks for a `"text"` key first, falls back to full JSON serialization.
    fn map_submission(
        &self,
        raw: &SubmissionResponse,
    ) -> Result<SubmissionContext, AggregatorError> {
        let content = Self::extract_content_text(raw.id, &raw.content)?;

        Ok(SubmissionContext {
            submission_id: raw.id,
            assignment_id: raw.assignment_id,
            user_id: raw.user_id,
            content,
            submitted_at: raw.created_at,
            previous_ai_result: raw.ai_result.clone(),
        })
    }

    /// Extracts a plain text string from the `content` JSONB field.
    ///
    /// Strategy:
    /// 1. If content has a `"text"` key, use its string value.
    /// 2. If content has an `"answer"` key, use its string value.
    /// 3. Fall back to serializing the whole JSON value as a string.
    ///    This handles freeform content maps gracefully.
    fn extract_content_text(
        submission_id: i32,
        content: &serde_json::Value,
    ) -> Result<String, AggregatorError> {
        if let Some(text) = content.get("text").and_then(|v| v.as_str()) {
            return Ok(text.to_string());
        }

        if let Some(answer) = content.get("answer").and_then(|v| v.as_str()) {
            return Ok(answer.to_string());
        }

        // Fallback: serialize the whole content map
        serde_json::to_string(content).map_err(|_| {
            AggregatorError::ContentExtractionFailed { submission_id }
        })
    }

    /// Parses the `rubric` JSONB field into a typed `Rubric` model.
    ///
    /// Attempts direct deserialization first. If that fails it means
    /// the rubric schema doesn't match — surfaces a clear error rather
    /// than panicking.
    fn parse_rubric(
        &self,
        assignment_id: i32,
        rubric_value: &serde_json::Value,
    ) -> Result<Rubric, AggregatorError> {
        serde_json::from_value::<Rubric>(rubric_value.clone()).map_err(|e| {
            AggregatorError::RubricParseFailed {
                assignment_id,
                reason: e.to_string(),
            }
        })
    }
}