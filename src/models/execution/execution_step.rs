use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// A single step within an `ExecutionPlan`.
///
/// Each step maps to exactly one tool invocation in the `ToolLayer`.
/// The `PlanExecutor` walks steps in `order` sequence, optionally
/// forwarding the `output` of one step as `parameters` input to the next.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub step_id: Uuid,
    /// Sequential position in the plan (1-indexed).
    pub order: u32,
    /// Tool name to invoke — must match a registered entry in `ToolRegistry`.
    pub tool_name: String,
    /// Input parameters for the tool, serialized as JSON.
    pub parameters: serde_json::Value,
    pub status: StepStatus,
    /// Populated by the executor after successful tool invocation.
    pub output: Option<serde_json::Value>,
    pub executed_at: Option<DateTime<Utc>>,
    /// Populated on failure — propagated to `ai_execution_steps.output_payload`.
    pub error: Option<String>,
}

impl ExecutionStep {
    pub fn new(order: u32, tool_name: &str, parameters: serde_json::Value) -> Self {
        Self {
            step_id: Uuid::new_v4(),
            order,
            tool_name: tool_name.to_string(),
            parameters,
            status: StepStatus::Pending,
            output: None,
            executed_at: None,
            error: None,
        }
    }

    /// Marks the step as running. Called by the executor before tool invocation.
    pub fn mark_running(&mut self) {
        self.status = StepStatus::Running;
    }

    /// Marks the step completed and stores the tool output.
    pub fn mark_completed(&mut self, output: serde_json::Value) {
        self.status = StepStatus::Completed;
        self.output = Some(output);
        self.executed_at = Some(chrono::Utc::now());
    }

    /// Marks the step failed and stores the error message.
    pub fn mark_failed(&mut self, error: String) {
        self.status = StepStatus::Failed;
        self.error = Some(error);
        self.executed_at = Some(chrono::Utc::now());
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            StepStatus::Completed | StepStatus::Failed | StepStatus::Skipped
        )
    }
}

/// Lifecycle status of a single execution step.
///
/// Persisted to `ai_execution_steps.status` in the AI database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    /// Reserved for conditional steps skipped by plan logic.
    Skipped,
}