use crate::models::grading::grading_result::GradingResult;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SuggestionCacheError {
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SuggestionCacheEntry {
    pub suggestion_id: Uuid,
    pub workspace_id: i32,
    pub assignment_id: i32,
    pub requester_user_id: String,
    pub created_at: DateTime<Utc>,
    pub results: Vec<GradingResult>,
}

pub struct SuggestionCache {
    client: Client,
    ttl: Duration,
}

impl SuggestionCache {
    pub fn from_env(ttl: Duration) -> Result<Self, SuggestionCacheError> {
        let redis_url = std::env::var("REDIS_URL")
            .map_err(|_| SuggestionCacheError::Configuration("REDIS_URL environment variable not set".to_string()))?;
        let client = Client::open(redis_url)?;
        Ok(Self { client, ttl })
    }

    pub async fn put(&self, entry: SuggestionCacheEntry) -> Result<(), SuggestionCacheError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = Self::key(&entry.suggestion_id);
        let payload = serde_json::to_string(&entry)?;
        let ttl_seconds = self.ttl.as_secs();
        conn.set_ex::<_, _, ()>(key, payload, ttl_seconds).await?;
        Ok(())
    }

    pub async fn get(&self, suggestion_id: &Uuid) -> Result<Option<SuggestionCacheEntry>, SuggestionCacheError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = Self::key(suggestion_id);
        let payload: Option<String> = conn.get(key).await?;
        match payload {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    pub async fn remove(&self, suggestion_id: &Uuid) -> Result<(), SuggestionCacheError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = Self::key(suggestion_id);
        conn.del::<_, ()>(key).await?;
        Ok(())
    }

    fn key(suggestion_id: &Uuid) -> String {
        format!("ai:suggestion:{suggestion_id}")
    }
}
