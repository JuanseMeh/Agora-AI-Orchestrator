use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// A single student submission and all data needed to evaluate it.
///
/// Assembled by the `ContextAggregationLayer` from the Workspace Service.
/// The `content` field is the raw submission body passed directly to the LLM
/// during criterion evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionContext {
    pub submission_id: i32,
    pub assignment_id: i32,
    pub user_id: i32,
    /// Raw submission content — essay text, code, or answer body.
    pub content: String,
    pub submitted_at: DateTime<Utc>,
    /// Previous AI result if this is a re-grade, preserved for audit purposes.
    pub previous_ai_result: Option<serde_json::Value>,
}

/// The unified context object passed into the grading pipeline.
///
/// The `ContextBuilder` assembles this before the orchestrator
/// begins execution plan resolution. Bundles one assignment with
/// all its pending submissions so the pipeline processes them together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingContext {
    pub assignment_id: i32,
    pub workspace_id: i32,
    pub submissions: Vec<SubmissionContext>,
}

impl GradingContext {
    pub fn new(assignment_id: i32, workspace_id: i32, submissions: Vec<SubmissionContext>) -> Self {
        Self {
            assignment_id,
            workspace_id,
            submissions,
        }
    }

    pub fn submission_count(&self) -> usize {
        self.submissions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.submissions.is_empty()
    }
}