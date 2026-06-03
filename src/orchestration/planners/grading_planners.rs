use chrono::Utc;
use uuid::Uuid;
use crate::models::execution::execution_plan::{ExecutionPlan, WorkflowType};
use crate::models::execution::execution_step::ExecutionStep;

/// Produces a static `ExecutionPlan` for a grading workflow.
///
/// The plan is deterministic — the same steps in the same order
/// every time. When `persist` is false the `save_grades` step is omitted
/// (used for the suggest-then-approve two-phase flow).
/// The `store_embeddings` step is included when RAG is enabled so that
/// grading results are indexed for future few-shot retrieval.
pub struct GradingPlanner;

impl GradingPlanner {
    /// Builds the execution plan for grading one assignment.
    pub fn build(
        workspace_id: i32,
        assignment_id: i32,
        persist: bool,
    ) -> ExecutionPlan {
        let mut step_order = 0u32;

        let mut steps = vec![
            ExecutionStep::new({
                step_order += 1; step_order
            }, "retrieve_assignment", serde_json::json!({
                "workspace_id": workspace_id,
                "assignment_id": assignment_id,
            })),
            ExecutionStep::new({
                step_order += 1; step_order
            }, "retrieve_submissions", serde_json::json!({
                "assignment_id": assignment_id,
            })),
            ExecutionStep::new({
                step_order += 1; step_order
            }, "evaluate_criteria", serde_json::json!({
                "assignment_id": assignment_id,
            })),
            ExecutionStep::new({
                step_order += 1; step_order
            }, "aggregate_scores", serde_json::json!({
                "assignment_id": assignment_id,
            })),
        ];

        if persist {
            steps.push(ExecutionStep::new({
                step_order += 1; step_order
            }, "save_grades", serde_json::json!({
                "assignment_id": assignment_id,
            })));
        }

        // Always include store_embeddings step — the workflow gracefully
        // skips it if RAG is disabled or no vector store is configured.
        steps.push(ExecutionStep::new({
            step_order += 1; step_order
        }, "store_embeddings", serde_json::json!({
            "assignment_id": assignment_id,
        })));

        ExecutionPlan {
            plan_id: Uuid::new_v4(),
            workflow_type: WorkflowType::GradingWorkflow,
            steps,
            expected_output: "grading_results".to_string(),
            created_at: Utc::now(),
        }
    }
}
