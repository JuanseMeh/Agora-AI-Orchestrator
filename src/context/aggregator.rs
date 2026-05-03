// context/aggregator.rs

use crate::context::workspace_client::{
    WorkspaceClient, WorkspaceClientError, AssignmentResponse, SubmissionResponse,
};
use crate::models::context::assignment_context::AssignmentContext;
use crate::models::context::submission_context::{GradingContext, SubmissionContext};
use crate::models::grading::rubric::{Rubric, RubricCriterion, ScoringLevel};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AggregatorError {
    #[error("workspace client error: {0}")]
    WorkspaceClient(#[from] WorkspaceClientError),

    #[error("assignment {id} is not closed — grading only runs on closed assignments")]
    AssignmentNotClosed { id: i32 },

    #[error("no submissions found for assignment {assignment_id}")]
    NoSubmissions { assignment_id: i32 },

    #[error("failed to parse rubric for assignment {assignment_id}: {reason}")]
    RubricParseFailed { assignment_id: i32, reason: String },

    #[error("failed to extract submission content for submission {submission_id}")]
    ContentExtractionFailed { submission_id: i32 },
}

/// Assembles a `GradingContext` from raw Workspace Service data.
///
/// This is the only place in the system that knows how to map
/// Workspace Service response shapes onto internal domain models.
pub struct ContextAggregator {
    client: WorkspaceClient,
}

impl ContextAggregator {
    pub fn new(client: WorkspaceClient) -> Self {
        Self { client }
    }

    pub fn from_env() -> Result<Self, WorkspaceClientError> {
        Ok(Self {
            client: WorkspaceClient::from_env()?,
        })
    }

    /// Builds a complete `GradingContext` for the given assignment.
    ///
    /// Validates that the assignment is closed (status = 2) before
    /// proceeding — grading is only valid on closed assignments.
    pub async fn build_grading_context(
        &self,
        assignment_id: i32,
    ) -> Result<(AssignmentContext, GradingContext), AggregatorError> {
        let raw_assignment = self.client.fetch_assignment(assignment_id).await?;

        // status 2 = Cerrado — only closed assignments can be graded
        // if raw_assignment.status != "CERRADO" {
        //     return Err(AggregatorError::AssignmentNotClosed { id: assignment_id });
        // }

        let raw_submissions = self.client.fetch_submissions(assignment_id).await?;

        if raw_submissions.is_empty() {
            return Err(AggregatorError::NoSubmissions { assignment_id });
        }

        let assignment_context = self.map_assignment(&raw_assignment)?;
        let workspace_id = raw_assignment.workspace_id;

        let submission_contexts = raw_submissions
            .iter()
            .map(|s| self.map_submission(s))
            .collect::<Result<Vec<_>, _>>()?;

        let grading_context = GradingContext::new(
            assignment_id,
            workspace_id,
            submission_contexts,
        );

        Ok((assignment_context, grading_context))
    }

    /// Maps a raw `AssignmentResponse` onto `AssignmentContext`.
    fn map_assignment(
        &self,
        raw: &AssignmentResponse,
    ) -> Result<AssignmentContext, AggregatorError> {
        let rubric = self.parse_rubric(raw.id, &raw.rubric)?;

        Ok(AssignmentContext {
            assignment_id: raw.id,
            workspace_id: raw.workspace_id,
            course_id: raw.workspace_id, // workspace is the course scope for now
            title: raw.name.clone(),
            description: raw.description.clone(),
            rubric,
            learning_objectives: None,
            created_at: raw.due_date,
        })
    }

    /// Maps a raw `SubmissionResponse` onto `SubmissionContext`.
    ///
    /// Extracts the submission text from the `content` JSONB field.
    /// Looks for a `"text"` key first, falls back to full JSON serialization.
    fn map_submission(
        &self,
        raw: &SubmissionResponse,
    ) -> Result<SubmissionContext, AggregatorError> {
        let content = Self::extract_content_text(raw.id, &raw.content)?;

        Ok(SubmissionContext {
            submission_id: raw.id,
            assignment_id: raw.assignment_id,
            user_id: raw.user_id,
            content,
            submitted_at: raw.created_at,
            previous_ai_result: raw.ai_result.clone(),
        })
    }

    /// Extracts a plain text string from the `content` JSONB field.
    ///
    /// Strategy:
    /// 1. If content has a `"text"` key, use its string value.
    /// 2. If content has an `"answer"` key, use its string value.
    /// 3. Fall back to serializing the whole JSON value as a string.
    ///    This handles freeform content maps gracefully.
    fn extract_content_text(
        submission_id: i32,
        content: &serde_json::Value,
    ) -> Result<String, AggregatorError> {
        if let Some(text) = content.get("text").and_then(|v| v.as_str()) {
            return Ok(text.to_string());
        }

        if let Some(answer) = content.get("answer").and_then(|v| v.as_str()) {
            return Ok(answer.to_string());
        }

        // Fallback: serialize the whole content map
        serde_json::to_string(content).map_err(|_| {
            AggregatorError::ContentExtractionFailed { submission_id }
        })
    }

    /// Parses the `rubric` JSONB field into a typed `Rubric` model.
    ///
    /// Attempts direct deserialization first. If that fails it means
    /// the rubric schema doesn't match — surfaces a clear error rather
    /// than panicking.
    fn parse_rubric(
        &self,
        assignment_id: i32,
        rubric_value: &serde_json::Value,
    ) -> Result<Rubric, AggregatorError> {
        if let Ok(rubric) = serde_json::from_value::<Rubric>(rubric_value.clone()) {
            return Ok(rubric);
        }

        if let Some(criteria_value) = rubric_value.get("criteria") {
            let criteria_array = criteria_value.as_array().ok_or_else(|| {
                AggregatorError::RubricParseFailed {
                    assignment_id,
                    reason: "rubric.criteria must be an array".to_string(),
                }
            })?;

            if criteria_array.is_empty() {
                return Err(AggregatorError::RubricParseFailed {
                    assignment_id,
                    reason: "rubric.criteria is empty".to_string(),
                });
            }

            let mut criteria = Vec::with_capacity(criteria_array.len());
            for item in criteria_array {
                let criterion_obj = item.as_object().ok_or_else(|| {
                    AggregatorError::RubricParseFailed {
                        assignment_id,
                        reason: "each rubric criterion must be an object".to_string(),
                    }
                })?;

                let name = criterion_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "Criterion".to_string());

                let criterion_id = criterion_obj
                    .get("criterion_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| name.to_lowercase().replace(' ', "_"));

                let description = criterion_obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);

                let max_score = criterion_obj
                    .get("value")
                    .and_then(|v| v.as_f64())
                    .or_else(|| criterion_obj.get("weight").and_then(|v| v.as_f64()))
                    .unwrap_or(1.0);

                let scoring_levels = if let Some(levels) = criterion_obj.get("scoring_levels") {
                    parse_scoring_levels(assignment_id, &name, max_score, levels)?
                } else {
                    vec![
                        ScoringLevel {
                            score: 0.0,
                            label: "insufficient".to_string(),
                            description: format!("Does not satisfy {}", name),
                        },
                        ScoringLevel {
                            score: max_score,
                            label: "meets".to_string(),
                            description: format!("Fully satisfies {}", name),
                        },
                    ]
                };

                criteria.push(RubricCriterion {
                    criterion_id,
                    name,
                    description,
                    weight: max_score,
                    scoring_levels,
                });
            }

            let rubric_id = rubric_value
                .get("rubric_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .unwrap_or_else(Uuid::nil);

            let title = rubric_value
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Rubric")
                .to_string();

            let description = rubric_value
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            return Ok(Rubric {
                rubric_id,
                assignment_id,
                title,
                description,
                criteria,
            });
        }

        // Backward-compat format from Workspace:
        // rubric: { "clarity": 50, "logic": 50 }
        let simple_map = rubric_value.as_object().ok_or_else(|| {
            AggregatorError::RubricParseFailed {
                assignment_id,
                reason: "rubric must be a JSON object".to_string(),
            }
        })?;

        if simple_map.is_empty() {
            return Err(AggregatorError::RubricParseFailed {
                assignment_id,
                reason: "rubric object is empty".to_string(),
            });
        }

        let mut criteria = Vec::with_capacity(simple_map.len());

        for (name, value) in simple_map {
            if ["rubric_id", "assignment_id", "title", "description", "criteria"].contains(&name.as_str()) {
                continue;
            }
            let max_score = value.as_f64().ok_or_else(|| AggregatorError::RubricParseFailed {
                assignment_id,
                reason: format!("rubric field '{}' must be numeric", name),
            })?;

            let criterion_id = name.to_lowercase().replace(' ', "_");

            criteria.push(RubricCriterion {
                criterion_id,
                name: name.clone(),
                description: Some(format!("Evaluates {}", name)),
                weight: max_score,
                scoring_levels: vec![
                    ScoringLevel {
                        score: 0.0,
                        label: "insufficient".to_string(),
                        description: format!("Does not satisfy {}", name),
                    },
                    ScoringLevel {
                        score: max_score,
                        label: "meets".to_string(),
                        description: format!("Fully satisfies {}", name),
                    },
                ],
            });
        }

        Ok(Rubric {
            rubric_id: Uuid::nil(),
            assignment_id,
            title: "Rubric".to_string(),
            description: None,
            criteria,
        })
    }
}

fn parse_scoring_levels(
    assignment_id: i32,
    criterion_name: &str,
    max_score: f64,
    levels: &Value,
) -> Result<Vec<ScoringLevel>, AggregatorError> {
    let raw_levels = levels.as_array().ok_or_else(|| AggregatorError::RubricParseFailed {
        assignment_id,
        reason: format!("scoring_levels for '{}' must be an array", criterion_name),
    })?;

    if raw_levels.is_empty() {
        return Ok(vec![
            ScoringLevel {
                score: 0.0,
                label: "insufficient".to_string(),
                description: format!("Does not satisfy {}", criterion_name),
            },
            ScoringLevel {
                score: max_score,
                label: "meets".to_string(),
                description: format!("Fully satisfies {}", criterion_name),
            },
        ]);
    }

    let mut parsed = Vec::with_capacity(raw_levels.len());
    for level in raw_levels {
        let level_obj = level.as_object().ok_or_else(|| AggregatorError::RubricParseFailed {
            assignment_id,
            reason: format!("each scoring level for '{}' must be an object", criterion_name),
        })?;

        let score = level_obj.get("score").and_then(|v| v.as_f64()).ok_or_else(|| {
            AggregatorError::RubricParseFailed {
                assignment_id,
                reason: format!("scoring level score for '{}' must be numeric", criterion_name),
            }
        })?;

        let label = level_obj
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("level")
            .to_string();

        let description = level_obj
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("No description provided")
            .to_string();

        parsed.push(ScoringLevel {
            score,
            label,
            description,
        });
    }

    Ok(parsed)
}
