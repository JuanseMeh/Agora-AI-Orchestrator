// application/workflows/grading/steps/aggregate_scores.rs

use chrono::Utc;
use uuid::Uuid;
use crate::models::grading::grading_result::{GradingResult, GradingStatus, CriterionResult};
use crate::models::context::submission_context::SubmissionContext;
use crate::models::grading::rubric::Rubric;

/// Aggregates a list of `CriterionResult` into a final `GradingResult`.
///
/// This step is purely computational — no LLM involvement.
/// Owned by the application layer, not the domain, because it
/// coordinates between multiple model types.
pub struct AggregateScores;

impl AggregateScores {
    /// Produces a `GradingResult` from evaluated criteria.
    ///
    /// Total score is the sum of all criterion scores.
    /// Max score is derived from the rubric definition.
    /// Feedback summary is assembled from individual criterion feedback.
    pub fn run(
        submission: &SubmissionContext,
        rubric: &Rubric,
        criteria_results: Vec<CriterionResult>,
        model: &str,
    ) -> GradingResult {
        let total_score: f64 = criteria_results.iter().map(|r| r.score).sum();
        let max_score = rubric.max_score();
        let feedback_summary = Self::build_feedback_summary(&criteria_results);

        GradingResult {
            result_id: Uuid::new_v4(),
            submission_id: submission.submission_id,
            assignment_id: submission.assignment_id,
            total_score,
            max_score,
            criteria_results,
            feedback_summary,
            evaluated_at: Utc::now(),
            grading_model: model.to_string(),
            status: GradingStatus::Completed,
        }
    }

    /// Builds a plain-text feedback summary from all criterion feedback entries.
    /// Each criterion contributes one line in the format: "CriterionName: feedback"
    fn build_feedback_summary(results: &[CriterionResult]) -> String {
        results
            .iter()
            .map(|r| format!("{}: {}", r.criterion_name, r.feedback))
            .collect::<Vec<_>>()
            .join("\n")
    }
}