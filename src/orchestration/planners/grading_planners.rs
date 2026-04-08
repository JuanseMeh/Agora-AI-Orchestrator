// orchestration/planners/grading_planner.rs

use chrono::Utc;
use uuid::Uuid;
use crate::models::execution::execution_plan::{ExecutionPlan, WorkflowType};
use crate::models::execution::execution_step::ExecutionStep;

/// Produces a static `ExecutionPlan` for a grading workflow.
///
/// The plan is deterministic — the same steps in the same order
/// every time. No LLM involvement in planning for grading.
pub struct GradingPlanner;

impl GradingPlanner {
    /// Builds the execution plan for grading one assignment.
    ///
    /// Steps mirror the static pipeline defined in the architecture docs:
    /// retrieve → evaluate → aggregate → save
    pub fn build(workspace_id: i32, assignment_id: i32) -> ExecutionPlan {
        let steps = vec![
            ExecutionStep::new(
                1,
                "retrieve_assignment",
                serde_json::json!({
                    "workspace_id": workspace_id,
                    "assignment_id": assignment_id,
                }),
            ),
            ExecutionStep::new(
                2,
                "retrieve_submissions",
                serde_json::json!({
                    "assignment_id": assignment_id,
                }),
            ),
            ExecutionStep::new(
                3,
                "evaluate_criteria",
                serde_json::json!({
                    "assignment_id": assignment_id,
                }),
            ),
            ExecutionStep::new(
                4,
                "aggregate_scores",
                serde_json::json!({
                    "assignment_id": assignment_id,
                }),
            ),
            ExecutionStep::new(
                5,
                "save_grades",
                serde_json::json!({
                    "assignment_id": assignment_id,
                }),
            ),
        ];

        ExecutionPlan {
            plan_id: Uuid::new_v4(),
            workflow_type: WorkflowType::GradingWorkflow,
            steps,
            expected_output: "grading_results".to_string(),
            created_at: Utc::now(),
        }
    }
}