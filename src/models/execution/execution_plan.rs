use super::execution_step::{ExecutionStep, StepStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub plan_id: Uuid,
    pub workflow_type: WorkflowType,
    pub steps: Vec<ExecutionStep>,
    pub expected_output: String,
    pub created_at: DateTime<Utc>,
}

impl ExecutionPlan {
    pub fn new(
        workflow_type: WorkflowType,
        steps: Vec<ExecutionStep>,
        expected_output: &str,
    ) -> Self {
        Self {
            plan_id: Uuid::new_v4(),
            workflow_type,
            steps,
            expected_output: expected_output.to_string(),
            created_at: Utc::now(),
        }
    }

    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// StepStatus is used here — no parallel vec needed.
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|s| s.status == StepStatus::Completed)
    }

    pub fn has_failures(&self) -> bool {
        self.steps.iter().any(|s| s.status == StepStatus::Failed)
    }

    pub fn pending_steps(&self) -> Vec<&ExecutionStep> {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Pending)
            .collect()
    }
}
/// The three workflow types the orchestrator can route to.
/// Maps directly to the three planner specializations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    /// Deterministic rubric-based grading pipeline.
    GradingWorkflow,
    /// Statistical class analytics pipeline.
    AnalyticsWorkflow,
    /// LLM-guided teaching recommendation agent.
    RecommendationWorkflow,
}

impl std::fmt::Display for WorkflowType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkflowType::GradingWorkflow => write!(f, "grading_workflow"),
            WorkflowType::AnalyticsWorkflow => write!(f, "analytics_workflow"),
            WorkflowType::RecommendationWorkflow => write!(f, "recommendation_workflow"),
        }
    }
}
