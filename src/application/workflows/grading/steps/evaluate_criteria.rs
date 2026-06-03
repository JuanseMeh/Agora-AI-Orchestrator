use std::sync::Arc;

use futures::future::try_join_all;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::application::prompts::criterion_prompt::CriterionPromptBuilder;
use crate::context::vector_store::VectorStoreHandle;
use crate::domain::ports::llm_provider::{LlmError, LlmProvider};
use crate::models::context::submission_context::SubmissionContext;
use crate::models::embedding::SimilarGradeExample;
use crate::models::grading::grading_result::CriterionResult;
use crate::models::grading::rubric::RubricCriterion;

fn rag_enabled() -> bool {
    std::env::var("RAG_ENABLED")
        .ok()
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(true)
}

fn rag_top_k() -> usize {
    std::env::var("RAG_TOP_K")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
}

/// Evaluates all rubric criteria for a single submission.
///
/// Evaluates every criterion against the submission in parallel,
/// with concurrency gated by a shared semaphore to avoid overloading
/// the LLM provider. Criterion order in the output matches input order.
///
/// When RAG is enabled and a vector store is available, semantically
/// similar past grading examples are retrieved and injected into the
/// prompt as few-shot references for more consistent grading.
pub struct EvaluateCriteria<'a> {
    provider: &'a dyn LlmProvider,
    semaphore: Arc<Semaphore>,
    vector_store: Option<VectorStoreHandle>,
}

impl<'a> EvaluateCriteria<'a> {
    pub fn new(
        provider: &'a dyn LlmProvider,
        semaphore: Arc<Semaphore>,
        vector_store: Option<VectorStoreHandle>,
    ) -> Self {
        Self { provider, semaphore, vector_store }
    }

    /// Evaluates every criterion in `criteria` against the submission content.
    pub async fn run(
        &self,
        criteria: &[RubricCriterion],
        submission: &SubmissionContext,
    ) -> Result<Vec<CriterionResult>, LlmError> {
        let content = &submission.content;

        let futs: Vec<_> = criteria.iter().map(|criterion| {
            let content = content.clone();
            let criterion_id = criterion.criterion_id.clone();
            let criterion_for_rag = criterion.clone();
            async move {
                let _permit = self.semaphore.acquire().await.expect("semaphore not closed");

                let examples = self.retrieve_similar(
                    &criterion_for_rag, &content,
                ).await;

                let prompt = if let Some(ref ex) = examples {
                    if ex.is_empty() {
                        CriterionPromptBuilder::build(&criterion_for_rag, &content)
                    } else {
                        CriterionPromptBuilder::build_with_rag(&criterion_for_rag, &content, ex)
                    }
                } else {
                    CriterionPromptBuilder::build(&criterion_for_rag, &content)
                };

                let result = self.provider.evaluate_criterion(prompt).await?;

                if let Some(ref ex) = examples {
                    if !ex.is_empty() {
                        info!(
                            criterion_id = %criterion_id,
                            example_count = ex.len(),
                            "grading with RAG few-shot context"
                        );
                    }
                }

                Ok(result)
            }
        }).collect();

        try_join_all(futs).await
    }

    /// Retrieves similar past grading examples via vector search.
    /// Returns `None` if RAG is disabled or no vector store is available.
    async fn retrieve_similar(
        &self,
        criterion: &RubricCriterion,
        submission_text: &str,
    ) -> Option<Vec<SimilarGradeExample>> {
        let store = self.vector_store.as_ref()?;
        if !rag_enabled() {
            return None;
        }

        let top_k = rag_top_k();

        // Build a combined text for similarity: criterion name + submission
        let embed_text = format!(
            "Criterion: {}\nDescription: {}\nSubmission: {}",
            criterion.name,
            criterion.description.as_deref().unwrap_or(""),
            submission_text.chars().take(2000).collect::<String>(),
        );

        let vector = match self.provider.embed_text(embed_text).await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "RAG embedding failed — falling back to no context");
                return Some(vec![]);
            }
        };

        match store.search_similar(&criterion.criterion_id, vector, top_k).await {
            Ok(examples) => {
                info!(
                    criterion_id = %criterion.criterion_id,
                    count = examples.len(),
                    "retrieved similar grading examples from Qdrant"
                );
                Some(examples)
            }
            Err(e) => {
                info!(
                    error = %e,
                    criterion_id = %criterion.criterion_id,
                    "no similar examples in Qdrant yet — first grading for this criterion"
                );
                Some(vec![])
            }
        }
    }

    /// Builds a text representation for embedding — used after grading
    /// to store the result in the vector store.
    pub fn build_embed_text(criterion: &RubricCriterion, submission_text: &str) -> String {
        format!(
            "Criterion: {}\nDescription: {}\nSubmission: {}",
            criterion.name,
            criterion.description.as_deref().unwrap_or(""),
            submission_text,
        )
    }

    /// Same as retrieve_similar but takes a pre-built embed text.
    pub async fn embed_for_store(
        provider: &dyn LlmProvider,
        text: String,
    ) -> Result<Vec<f32>, LlmError> {
        provider.embed_text(text).await
    }
}
