use std::collections::HashSet;

use serde_json::Value;
use thiserror::Error;
use tracing::warn;
use uuid::Uuid;

use crate::context::media_client::{MediaClientError, MediaServiceClient};
use crate::context::workspace_client::{
    AssignmentResponse, SubmissionResponse, WorkspaceClient, WorkspaceClientError,
};
use crate::models::context::assignment_context::AssignmentContext;
use crate::models::context::submission_context::{GradingContext, SubmissionContext};
use crate::models::grading::rubric::{Rubric, RubricCriterion, ScoringLevel};

#[derive(Debug, Error)]
pub enum AggregatorError {
    #[error("workspace client error: {0}")]
    WorkspaceClient(#[from] WorkspaceClientError),

    #[error("media client error: {0}")]
    MediaClient(#[from] MediaClientError),

    #[error("assignment {id} is not closed — grading only runs on closed assignments")]
    AssignmentNotClosed { id: i32 },

    #[error("no submissions found for assignment {assignment_id}")]
    NoSubmissions { assignment_id: i32 },

    #[error("failed to parse rubric for assignment {assignment_id}: {reason}")]
    RubricParseFailed { assignment_id: i32, reason: String },

    #[error("failed to extract submission content for submission {submission_id}")]
    ContentExtractionFailed { submission_id: i32 },

    #[error("invalid user id filter: {value}")]
    InvalidUserFilter { value: String },
}

#[derive(Debug, Clone, Default)]
pub struct SubmissionFilter {
    pub submission_ids: Vec<i32>,
    pub user_ids: Vec<Uuid>,
    pub include_already_graded: bool,
}

/// Assembles a `GradingContext` from raw Workspace Service data.
///
/// This is the only place in the system that knows how to map
/// Workspace Service response shapes onto internal domain models.
pub struct ContextAggregator {
    client: WorkspaceClient,
    media_client: MediaServiceClient,
}

impl ContextAggregator {
    pub fn new(client: WorkspaceClient, media_client: MediaServiceClient) -> Self {
        Self { client, media_client }
    }

    pub fn from_env() -> Result<Self, WorkspaceClientError> {
        let client = WorkspaceClient::from_env()?;
        let media_client = MediaServiceClient::from_env().map_err(|e| {
            WorkspaceClientError::Configuration(e.to_string())
        })?;
        Ok(Self { client, media_client })
    }

    /// Builds a complete `GradingContext` for the given assignment.
    pub async fn build_grading_context(
        &self,
        assignment_id: i32,
    ) -> Result<(AssignmentContext, GradingContext), AggregatorError> {
        self.build_grading_context_with_filter(assignment_id, &SubmissionFilter::default())
            .await
    }

    pub async fn build_grading_context_with_filter(
        &self,
        assignment_id: i32,
        filter: &SubmissionFilter,
    ) -> Result<(AssignmentContext, GradingContext), AggregatorError> {
        let raw_assignment = self.client.fetch_assignment(assignment_id).await?;
        let raw_submissions = self.client.fetch_submissions(assignment_id).await?;
        let filtered_submissions = self.apply_submission_filter(raw_submissions, filter);

        let assignment_context = self.map_assignment(&raw_assignment).await?;
        let workspace_id = raw_assignment.workspace_id;

        let mut submission_contexts = Vec::with_capacity(filtered_submissions.len());
        for s in &filtered_submissions {
            submission_contexts.push(self.map_submission(s).await?);
        }

        if submission_contexts.is_empty() {
            return Err(AggregatorError::NoSubmissions { assignment_id });
        }

        let grading_context = GradingContext::new(assignment_id, workspace_id, submission_contexts);

        Ok((assignment_context, grading_context))
    }

    pub fn parse_user_filters(values: &[String]) -> Result<Vec<Uuid>, AggregatorError> {
        values
            .iter()
            .map(|value| {
                Uuid::parse_str(value)
                    .map_err(|_| AggregatorError::InvalidUserFilter { value: value.clone() })
            })
            .collect()
    }

    fn apply_submission_filter(
        &self,
        submissions: Vec<SubmissionResponse>,
        filter: &SubmissionFilter,
    ) -> Vec<SubmissionResponse> {
        let submission_id_set: HashSet<i32> = filter.submission_ids.iter().copied().collect();
        let user_id_set: HashSet<Uuid> = filter.user_ids.iter().copied().collect();

        submissions
            .into_iter()
            .filter(|s| {
                if !submission_id_set.is_empty() && !submission_id_set.contains(&s.id) {
                    return false;
                }
                if !user_id_set.is_empty() && !user_id_set.contains(&s.user_id) {
                    return false;
                }
                if !filter.include_already_graded && Self::is_already_graded(s.ai_result.as_ref())
                {
                    return false;
                }
                true
            })
            .collect()
    }

    fn is_already_graded(ai_result: Option<&Value>) -> bool {
        let Some(result) = ai_result else {
            return false;
        };
        let has_ai_score = result
            .get("ai")
            .and_then(|ai| ai.get("score"))
            .is_some();
        let has_teacher_score = result
            .get("teacher")
            .and_then(|t| t.get("score"))
            .is_some();
        let ai_approved = result
            .get("aiStatus")
            .and_then(|s| s.as_str())
            .map_or(false, |s| s == "APPROVED");
        has_ai_score || ai_approved || has_teacher_score
    }

    /// Extracts `mediaId` values from a JSONB attachments array.
    ///
    /// Workspace service stores submission files and assignment settings
    /// as `{ "attachments": [{ "mediaId": "uuid", ... }, ...] }`.
    /// Returns all mediaId strings found, or an empty vec.
    fn extract_media_ids(value: &Option<Value>) -> Vec<String> {
        let Some(val) = value else { return vec![] };
        let Some(attachments) = val.get("attachments") else { return vec![] };
        let Some(array) = attachments.as_array() else { return vec![] };
        array
            .iter()
            .filter_map(|item| item.get("mediaId").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect()
    }

    /// Concatenates processed text items into a single block with separators.
    fn join_file_texts(items: &[crate::context::media_client::BatchProcessItem]) -> Option<String> {
        if items.is_empty() {
            return None;
        }
        let parts: Vec<String> = items
            .iter()
            .map(|item| {
                format!(
                    "--- File: {} (type: {}) ---\n{}",
                    item.original_filename, item.mime_type, item.text
                )
            })
            .collect();
        Some(parts.join("\n\n"))
    }

    /// Maps a raw `AssignmentResponse` onto `AssignmentContext`.
    ///
    /// Fetches processed text from any files attached to the assignment
    /// (e.g. reference documents) via the Media Service.
    async fn map_assignment(
        &self,
        raw: &AssignmentResponse,
    ) -> Result<AssignmentContext, AggregatorError> {
        let rubric = self.parse_rubric(raw.id, &raw.rubric)?;

        let grading_scale = raw.settings
            .as_ref()
            .and_then(|s| s.get("grading_scale"))
            .and_then(|v| v.as_f64());

        let teacher_instructions = raw.settings
            .as_ref()
            .and_then(|s| s.get("teacher_instructions"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        let mut assignment = AssignmentContext {
            assignment_id: raw.id,
            workspace_id: raw.workspace_id,
            course_id: raw.workspace_id,
            title: raw.name.clone(),
            description: raw.description.clone(),
            rubric,
            learning_objectives: None,
            assignment_attachments_content: None,
            grading_scale,
            teacher_instructions,
            created_at: raw.due_date,
        };

        let media_ids = Self::extract_media_ids(&raw.settings);
        if !media_ids.is_empty() {
            match self.media_client.batch_process(media_ids).await {
                Ok(items) => {
                    assignment.assignment_attachments_content = Self::join_file_texts(&items);
                    if assignment.assignment_attachments_content.is_some() {
                        tracing::info!(
                            assignment_id = raw.id,
                            file_count = items.len(),
                            "fetched assignment attachment content"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        assignment_id = raw.id,
                        "failed to fetch assignment attachment content — continuing without it"
                    );
                }
            }
        }

        Ok(assignment)
    }

    /// Maps a raw `SubmissionResponse` onto `SubmissionContext`.
    ///
    /// Extracts the submission text from the `content` JSONB field and
    /// fetches processed text from any files the student attached.
    async fn map_submission(
        &self,
        raw: &SubmissionResponse,
    ) -> Result<SubmissionContext, AggregatorError> {
        let content = Self::extract_content_text(raw.id, &raw.content)?;

        let mut submission = SubmissionContext {
            submission_id: raw.id,
            assignment_id: raw.assignment_id,
            user_id: raw.user_id,
            content,
            submission_files_content: None,
            submitted_at: raw.created_at,
            previous_ai_result: raw.ai_result.clone(),
        };

        let media_ids = Self::extract_media_ids(&raw.files);
        if !media_ids.is_empty() {
            match self.media_client.batch_process(media_ids).await {
                Ok(items) => {
                    submission.submission_files_content = Self::join_file_texts(&items);
                    if submission.submission_files_content.is_some() {
                        tracing::info!(
                            submission_id = raw.id,
                            file_count = items.len(),
                            "fetched submission file content"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        submission_id = raw.id,
                        "failed to fetch submission file content — continuing without it"
                    );
                }
            }
        }

        Ok(submission)
    }

    /// Extracts a plain text string from the `content` JSONB field.
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

        serde_json::to_string(content).map_err(|_| {
            AggregatorError::ContentExtractionFailed { submission_id }
        })
    }

    /// Parses the `rubric` JSONB field into a typed `Rubric` model.
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
            if ["rubric_id", "assignment_id", "title", "description", "criteria"]
                .contains(&name.as_str())
            {
                continue;
            }
            let max_score = value.as_f64().ok_or_else(|| {
                AggregatorError::RubricParseFailed {
                    assignment_id,
                    reason: format!("rubric field '{}' must be numeric", name),
                }
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
    let raw_levels = levels.as_array().ok_or_else(|| {
        AggregatorError::RubricParseFailed {
            assignment_id,
            reason: format!("scoring_levels for '{}' must be an array", criterion_name),
        }
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
        let level_obj = level.as_object().ok_or_else(|| {
            AggregatorError::RubricParseFailed {
                assignment_id,
                reason: format!(
                    "each scoring level for '{}' must be an object",
                    criterion_name
                ),
            }
        })?;

        let score = level_obj.get("score").and_then(|v| v.as_f64()).ok_or_else(
            || {
                AggregatorError::RubricParseFailed {
                    assignment_id,
                    reason: format!(
                        "scoring level score for '{}' must be numeric",
                        criterion_name
                    ),
                }
            },
        )?;

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
