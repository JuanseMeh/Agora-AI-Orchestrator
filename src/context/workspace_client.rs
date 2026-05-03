// context/workspace_client.rs

use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkspaceClientError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("assignment not found: {id}")]
    AssignmentNotFound { id: i32 },

    #[error("submissions not found for assignment: {assignment_id}")]
    SubmissionsNotFound { assignment_id: i32 },

    #[error("workspace service error {status}: {message}")]
    ServiceError { status: u16, message: String },

    #[error("failed to deserialize response: {0}")]
    Deserialization(String),

    #[error("configuration error: {0}")]
    Configuration(String),
}

/// Raw assignment shape returned by the Workspace Service.
/// Maps directly onto `GET /internal/assignments/{id}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentResponse {
    pub id: i32,
    pub workspace_id: i32,
    pub name: String,
    pub description: String,
    pub due_date: DateTime<Utc>,
    pub status: String,
    pub rubric: Value,
    pub settings: Option<Value>,
    pub is_expired: bool,
}

/// Raw submission shape returned by the Workspace Service.
/// Maps directly onto `GET /internal/submissions/assignment/{id}`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionResponse {
    pub id: i32,
    pub assignment_id: i32,
    pub user_id: uuid::Uuid,
    pub created_at: DateTime<Utc>,
    pub content: Value,
    pub files: Option<Value>,
    #[serde(default, alias = "result")]
    pub ai_result: Option<Value>,
}

/// Payload sent to `PUT /internal/submissions/grade/{id}`.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GradeWritePayload {
    pub score: f64,
    pub feedback: String,
    pub rubric_results: Vec<CriterionResultPayload>,
    pub evaluated_at: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionResultPayload {
    pub criterion_id: String,
    pub score: f64,
    pub feedback: String,
}

/// HTTP client scoped to the Workspace Service.
///
/// Reads base URL once from `WORKSPACE_SERVICE_URL` at construction.
/// Stateless beyond that — safe to wrap in `Arc`.
pub struct WorkspaceClient {
    client: Client,
    base_url: String,
}

impl WorkspaceClient {
    pub fn from_env() -> Result<Self, WorkspaceClientError> {
        let base_url = std::env::var("WORKSPACE_SERVICE_URL")
            .map_err(|_| WorkspaceClientError::Configuration(
                "WORKSPACE_SERVICE_URL environment variable not set".to_string()
            ))?;

        Ok(Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Fetches a single assignment by ID.
    pub async fn fetch_assignment(
        &self,
        assignment_id: i32,
    ) -> Result<AssignmentResponse, WorkspaceClientError> {
        let url = format!("{}/internal/assignments/{}", self.base_url, assignment_id);

        let response = self.client.get(&url).send().await?;
        let status = response.status();

        if status.as_u16() == 404 {
            return Err(WorkspaceClientError::AssignmentNotFound { id: assignment_id });
        }

        if !status.is_success() {
            return Err(WorkspaceClientError::ServiceError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        response
            .json::<AssignmentResponse>()
            .await
            .map_err(|e| WorkspaceClientError::Deserialization(e.to_string()))
    }

    /// Fetches all submissions for a given assignment.
    pub async fn fetch_submissions(
        &self,
        assignment_id: i32,
    ) -> Result<Vec<SubmissionResponse>, WorkspaceClientError> {
        let url = format!(
            "{}/internal/submissions/assignment/{}",
            self.base_url, assignment_id
        );

        let response = self.client.get(&url).send().await?;
        let status = response.status();

        if status.as_u16() == 404 {
            return Err(WorkspaceClientError::SubmissionsNotFound { assignment_id });
        }

        if !status.is_success() {
            return Err(WorkspaceClientError::ServiceError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        response
            .json::<Vec<SubmissionResponse>>()
            .await
            .map_err(|e| WorkspaceClientError::Deserialization(e.to_string()))
    }

    /// Writes an AI grading result back to a submission.
    pub async fn write_grade(
        &self,
        submission_id: i32,
        payload: GradeWritePayload,
    ) -> Result<(), WorkspaceClientError> {
        let url = format!(
            "{}/internal/submissions/grade/{}",
            self.base_url, submission_id
        );

        let response = self.client.put(&url).json(&payload).send().await?;
        let status = response.status();

        if !status.is_success() {
            return Err(WorkspaceClientError::ServiceError {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        Ok(())
    }

}
