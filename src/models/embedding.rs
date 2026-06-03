use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A vector embedding of a submission + grading result pair.
///
/// Stored in Qdrant and used for few-shot RAG retrieval:
/// when grading a new submission, the most semantically similar
/// past grading examples are injected into the LLM prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingEmbedding {
    pub point_id: Uuid,
    pub submission_id: i32,
    pub assignment_id: i32,
    pub workspace_id: i32,
    pub criterion_id: String,
    pub submission_text: String,
    pub score: f64,
    pub max_score: f64,
    pub feedback: String,
    pub matched_level: String,
}

/// A similar past grading example retrieved from Qdrant,
/// formatted for few-shot injection into the LLM prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarGradeExample {
    pub score: f64,
    pub max_score: f64,
    pub feedback: String,
    pub matched_level: String,
    pub similarity_score: f64,
}

/// Text to embed — either a submission or a combined string
/// for similarity search.
#[derive(Debug, Clone)]
pub struct EmbeddingInput {
    pub text: String,
}

/// The response from an embedding API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embedding: Vec<f32>,
    pub model: String,
}

/// Configuration for the vector store.
#[derive(Debug, Clone)]
pub struct VectorStoreConfig {
    pub url: String,
    pub collection_name: String,
    pub embedding_size: usize,
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        Self {
            url: std::env::var("QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6333".to_string()),
            collection_name: "grading_examples".to_string(),
            embedding_size: 768,
        }
    }
}
