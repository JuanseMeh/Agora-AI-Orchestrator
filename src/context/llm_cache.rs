use std::sync::Arc;
use std::time::Duration;

use redis::AsyncCommands;
use redis::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmCacheError {
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("deserialization error: {0}")]
    Deserialization(String),
}

/// A Redis-backed cache for LLM responses.
///
/// Each cached entry is keyed by a logical cache key (e.g.
/// `"ai:llm:{submission_id}:{criterion_id}"`) and stored as
/// JSON with a configurable TTL.
///
/// On cache hit the LLM call is skipped entirely — including
/// semaphore acquisition — saving both money and latency.
pub struct LlmCache {
    client: Client,
    ttl: Duration,
}

impl LlmCache {
    pub fn from_env(ttl: Duration) -> Result<Self, LlmCacheError> {
        let redis_url = std::env::var("REDIS_URL")
            .map_err(|_| LlmCacheError::Configuration("REDIS_URL is not set".into()))?;
        let client = Client::open(redis_url)?;
        Ok(Self { client, ttl })
    }

    /// Returns the cached value if present and valid.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, LlmCacheError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let payload: Option<String> = conn.get(key).await?;
        match payload {
            Some(json) => {
                let value = serde_json::from_str(&json)
                    .map_err(|e| LlmCacheError::Deserialization(e.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Stores a value in the cache with the configured TTL.
    pub async fn put<T: Serialize>(&self, key: &str, value: &T) -> Result<(), LlmCacheError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let payload = serde_json::to_string(value)?;
        let ttl_secs = self.ttl.as_secs();
        conn.set_ex::<_, _, ()>(key, payload, ttl_secs).await?;
        Ok(())
    }

    /// Builds a standard cache key from its logical parts.
    pub fn build_key(submission_id: i32, criterion_id: &str) -> String {
        format!("ai:llm:{}:{}", submission_id, criterion_id)
    }

    /// Invalidates a specific entry.
    pub async fn invalidate(&self, key: &str) -> Result<(), LlmCacheError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        conn.del::<_, ()>(key).await?;
        Ok(())
    }
}

pub type LlmCacheHandle = Arc<LlmCache>;
