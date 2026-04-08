use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// The complete output of the grading engine for one submission.
/// This is what gets written back to `submission.ai_result JSONB`
/// in the Workspace Service database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingResult {
    pub result_id: Uuid,
    pub submission_id: i32,
    pub assignment_id: i32,
    /// Total aggregated score across all criteria.
    pub total_score: f64,
    /// Maximum possible score, derived from the rubric.
    pub max_score: f64,
    /// Per-criterion breakdown.
    pub criteria_results: Vec<CriterionResult>,
    /// High-level qualitative summary for the student.
    pub feedback_summary: String,
    pub evaluated_at: DateTime<Utc>,
    pub grading_model: String,
    pub status: GradingStatus,
}

impl GradingResult {
    /// Returns the score as a percentage (0.0–100.0).
    pub fn score_percentage(&self) -> f64 {
        if self.max_score == 0.0 {
            return 0.0;
        }
        (self.total_score / self.max_score) * 100.0
    }

    /// Serializes to the JSONB structure expected by the Workspace Service.
    /// Matches the `submission.ai_result` schema defined in the persistence docs.
    pub fn to_workspace_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "score": self.total_score,
            "criteria": self.criteria_results.iter().map(|c| serde_json::json!({
                "criterion_id": c.criterion_id,
                "score": c.score,
                "feedback": c.feedback,
            })).collect::<Vec<_>>(),
            "feedback_summary": self.feedback_summary,
            "evaluated_at": self.evaluated_at.to_rfc3339(),
        })
    }
}

/// The AI evaluation result for a single rubric criterion.
/// Produced by the `CriterionEvaluator` in the grading engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionResult {
    pub criterion_id: String,
    pub criterion_name: String,
    /// Score awarded, must fall within one of the rubric's `ScoringLevel` values.
    pub score: f64,
    /// Maximum score possible for this criterion.
    pub max_score: f64,
    /// Qualitative explanation tied to the rubric level awarded.
    pub feedback: String,
    /// The rubric level label that was matched (e.g. "Proficient", "Needs Work").
    pub matched_level: String,
}

/// Represents the grading status of a submission through the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GradingStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}