use serde::{Deserialize, Serialize};

/// Structured feedback produced at the end of the grading pipeline.
/// Wraps the `GradingResult` into a student-facing communication artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackSummary {
    /// Overall qualitative assessment narrative.
    pub overall_narrative: String,
    /// Specific areas where the student demonstrated strength.
    pub strengths: Vec<String>,
    /// Specific areas the student should focus on improving.
    pub improvement_areas: Vec<String>,
    /// Optional actionable next steps recommended by the AI.
    pub recommendations: Option<Vec<String>>,
}

impl FeedbackSummary {
    /// Renders the summary as a human-readable string.
    /// Useful for plain-text delivery channels.
    pub fn to_plain_text(&self) -> String {
        let mut parts = vec![self.overall_narrative.clone()];

        if !self.strengths.is_empty() {
            parts.push(format!(
                "Strengths:\n{}",
                self.strengths
                    .iter()
                    .map(|s| format!("- {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if !self.improvement_areas.is_empty() {
            parts.push(format!(
                "Areas for improvement:\n{}",
                self.improvement_areas
                    .iter()
                    .map(|a| format!("- {}", a))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if let Some(recs) = &self.recommendations {
            parts.push(format!(
                "Recommendations:\n{}",
                recs.iter()
                    .map(|r| format!("- {}", r))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        parts.join("\n\n")
    }
}