use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A complete rubric attached to an assignment.
/// Rubrics are owned by the Workspace Service and stored in `assignment.rubric JSONB`.
/// The AI module receives this as input — it never stores rubrics itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rubric {
    pub rubric_id: Uuid,
    pub assignment_id: i32,
    pub title: String,
    pub description: Option<String>,
    pub criteria: Vec<RubricCriterion>,
}

impl Rubric {
    /// Returns the maximum achievable score across all criteria.
    pub fn max_score(&self) -> f64 {
        self.criteria.iter().map(|c| c.max_score()).sum()
    }
}

/// A single evaluatable criterion within a rubric.
/// Each criterion is evaluated independently by the grading engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricCriterion {
    pub criterion_id: String,
    pub name: String,
    pub description: String,
    /// Relative importance of this criterion in the final grade.
    pub weight: f64,
    /// Ordered scoring levels from lowest to highest performance.
    pub scoring_levels: Vec<ScoringLevel>,
}

impl RubricCriterion {
    /// Returns the highest score achievable for this criterion.
    pub fn max_score(&self) -> f64 {
        self.scoring_levels
            .iter()
            .map(|l| l.score)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Returns the scoring level descriptors as a formatted string
    /// suitable for inclusion in an LLM prompt.
    pub fn levels_as_prompt_context(&self) -> String {
        self.scoring_levels
            .iter()
            .map(|l| format!("- Score {}: {}", l.score, l.description))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A discrete performance level within a criterion.
/// These drive deterministic rubric constraints in the LLM prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringLevel {
    pub score: f64,
    pub label: String,
    pub description: String,
}