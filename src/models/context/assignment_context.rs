use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::models::grading::rubric::Rubric;

/// All assignment-level data the AI module needs to perform grading.
/// Assembled by the `ContextAggregationLayer` from the Workspace Service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentContext {
    pub assignment_id: i32,
    pub workspace_id: i32,
    pub course_id: i32,
    pub title: String,
    pub description: String,
    /// The structured rubric retrieved from `assignment.rubric JSONB`.
    pub rubric: Rubric,
    /// Optional learning objectives to give the LLM additional grading context.
    pub learning_objectives: Option<Vec<String>>,
    /// Processed text content from files attached to this assignment
    /// (e.g. reference documents, templates). Extracted by the Media
    /// Service and concatenated for LLM context.
    pub assignment_attachments_content: Option<String>,
    /// Grading scale max value (e.g. 5.0 for Colombian 0.0–5.0 scale).
    /// Read from assignment.settings.grading_scale. Defaults to rubric.max_score().
    pub grading_scale: Option<f64>,
    /// Free-form instructions from the teacher injected verbatim into the LLM prompt.
    /// Read from assignment.settings.teacher_instructions.
    pub teacher_instructions: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl AssignmentContext {
    pub fn attachments_text(&self) -> Option<&str> {
        self.assignment_attachments_content.as_deref()
    }
}
