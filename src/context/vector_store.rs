#![allow(dead_code)]

use std::sync::Arc;

use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;
use tracing::{error, info};

use crate::models::embedding::{GradingEmbedding, SimilarGradeExample, VectorStoreConfig};

#[derive(Debug, Error)]
pub enum VectorStoreError {
    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("qdrant API error: {status} — {message}")]
    Api { status: u16, message: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("no similar examples found")]
    NoResults,
}

/// A Qdrant point as returned by the search API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchedPoint {
    id: String,
    score: f32,
    payload: serde_json::Value,
}

/// Wraps Qdrant's REST API for storing and retrieving
/// grading example embeddings.
///
/// Each point stores:
/// - submission text + criterion → embedded as a vector
/// - payload: the grading result (score, feedback, etc.)
///
/// On retrieval, the top-K most semantically similar past
/// grading examples are returned for few-shot RAG injection.
pub struct VectorStore {
    client: Client,
    config: VectorStoreConfig,
}

impl VectorStore {
    pub fn new(config: VectorStoreConfig) -> Result<Self, VectorStoreError> {
        let client = Client::builder()
            .no_proxy()
            .build()?;

        Ok(Self { client, config })
    }

    pub fn from_env() -> Result<Self, VectorStoreError> {
        Self::new(VectorStoreConfig::default())
    }

    /// Ensures the collection exists. Idempotent — safe to call on every startup.
    pub async fn ensure_collection(&self) -> Result<(), VectorStoreError> {
        let exists = self.collection_exists().await?;
        if exists {
            info!(collection = %self.config.collection_name, "vector collection already exists");
            return Ok(());
        }

        let body = json!({
            "vectors": {
                "size": self.config.embedding_size,
                "distance": "Cosine"
            }
        });

        let url = format!("{}/collections/{}", self.config.url, self.config.collection_name);
        let resp = self.client.put(&url).json(&body).send().await.map_err(|e| {
            error!(error = %e, "failed to create qdrant collection");
            VectorStoreError::Http(e)
        })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "qdrant create collection failed");
            return Err(VectorStoreError::Api { status: status.as_u16(), message: text });
        }

        info!(collection = %self.config.collection_name, "vector collection created");
        Ok(())
    }

    async fn collection_exists(&self) -> Result<bool, VectorStoreError> {
        let url = format!("{}/collections/{}", self.config.url, self.config.collection_name);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.status().is_success())
    }

    /// Stores a grading embedding point in Qdrant.
    pub async fn upsert_point(&self, embedding: &GradingEmbedding, vector: Vec<f32>) -> Result<(), VectorStoreError> {
        let point_id = embedding.point_id.to_string();
        let payload = json!({
            "submission_id": embedding.submission_id,
            "assignment_id": embedding.assignment_id,
            "workspace_id": embedding.workspace_id,
            "criterion_id": embedding.criterion_id,
            "submission_text": embedding.submission_text,
            "score": embedding.score,
            "max_score": embedding.max_score,
            "feedback": embedding.feedback,
            "matched_level": embedding.matched_level,
        });

        let body = json!({
            "points": [{
                "id": point_id,
                "vector": vector,
                "payload": payload
            }]
        });

        let url = format!("{}/collections/{}/points", self.config.url, self.config.collection_name);
        let resp = self.client.put(&url).json(&body).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "qdrant upsert point failed");
            return Err(VectorStoreError::Api { status: status.as_u16(), message: text });
        }

        Ok(())
    }

    /// Stores a teacher-corrected grading example in Qdrant.
    ///
    /// These points are tagged with `teacher_corrected: true` so they
    /// can be boosted or distinguished from AI-generated points during
    /// RAG retrieval.
    pub async fn store_teacher_correction(
        &self,
        point_id: &str,
        submission_id: i32,
        assignment_id: i32,
        workspace_id: i32,
        criterion_id: &str,
        submission_text: &str,
        score: f64,
        max_score: f64,
        feedback: &str,
        matched_level: &str,
        vector: Vec<f32>,
    ) -> Result<(), VectorStoreError> {
        let payload = json!({
            "submission_id": submission_id,
            "assignment_id": assignment_id,
            "workspace_id": workspace_id,
            "criterion_id": criterion_id,
            "submission_text": submission_text,
            "score": score,
            "max_score": max_score,
            "feedback": feedback,
            "matched_level": matched_level,
            "teacher_corrected": true,
        });

        let body = json!({
            "points": [{
                "id": point_id,
                "vector": vector,
                "payload": payload
            }]
        });

        let url = format!("{}/collections/{}/points", self.config.url, self.config.collection_name);
        let resp = self.client.put(&url).json(&body).send().await?;
        let status = resp.status();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "qdrant store correction failed");
            return Err(VectorStoreError::Api { status: status.as_u16(), message: text });
        }

        Ok(())
    }

    /// Searches for the top-K most similar past grading examples.
    ///
    /// `criterion_id` filters results to the same criterion (so examples
    /// match the grading dimension). `submission_vector` is the embedding
    /// of the current submission text for similarity comparison.
    pub async fn search_similar(
        &self,
        criterion_id: &str,
        submission_vector: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<SimilarGradeExample>, VectorStoreError> {
        let body = json!({
            "vector": submission_vector,
            "limit": top_k,
            "with_payload": true,
            "filter": {
                "must": [
                    { "key": "criterion_id", "match": { "value": criterion_id } }
                ]
            }
        });

        let url = format!(
            "{}/collections/{}/points/search",
            self.config.url,
            self.config.collection_name,
        );

        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            if status.as_u16() == 404 {
                // Collection doesn't exist yet — no results
                return Err(VectorStoreError::NoResults);
            }
            error!(status = %status, body = %text, "qdrant search failed");
            return Err(VectorStoreError::Api { status: status.as_u16(), message: text });
        }

        #[derive(Deserialize)]
        struct SearchResult {
            result: Vec<SearchedPoint>,
        }

        let search_result: SearchResult = resp.json().await?;

        let examples: Vec<SimilarGradeExample> = search_result.result.into_iter().map(|pt| SimilarGradeExample {
            score: pt.payload.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0),
            max_score: pt.payload.get("max_score").and_then(|v| v.as_f64()).unwrap_or(0.0),
            feedback: pt.payload.get("feedback").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            matched_level: pt.payload.get("matched_level").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            similarity_score: pt.score as f64,
        }).collect();

        if examples.is_empty() {
            return Err(VectorStoreError::NoResults);
        }

        Ok(examples)
    }

    /// Deletes points for a given submission (e.g. if re-grading).
    pub async fn delete_submission_points(&self, submission_id: i32) -> Result<(), VectorStoreError> {
        let body = json!({
            "filter": {
                "must": [
                    { "key": "submission_id", "match": { "value": submission_id } }
                ]
            }
        });

        let url = format!(
            "{}/collections/{}/points/delete",
            self.config.url,
            self.config.collection_name,
        );

        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "qdrant delete points failed");
            return Err(VectorStoreError::Api { status: status.as_u16(), message: text });
        }

        Ok(())
    }
}

/// Thread-safe handle for the vector store.
pub type VectorStoreHandle = Arc<VectorStore>;
