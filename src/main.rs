#![allow(dead_code)]
#![allow(unused_imports)]

mod api;
mod application;
mod context;
mod domain;
mod integration;
mod models;
mod orchestration;

use std::sync::Arc;
use integration::gemini::client::GeminiClient;
use orchestration::orchestrator::Orchestrator;
use context::aggregator::ContextAggregator;
use context::suggestion_cache::SuggestionCache;
use context::user_config_client::UserConfigClient;
use context::workspace_client::WorkspaceClient;
use dotenvy;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

        dotenvy::dotenv().ok();

    let provider = Arc::new(GeminiClient::from_env()?);
    let orchestrator = Arc::new(Orchestrator::new(provider.clone()));
    let aggregator = Arc::new(ContextAggregator::from_env()?);
    let workspace_client = Arc::new(WorkspaceClient::from_env()?);
    let user_config_client = Arc::new(UserConfigClient::from_env()?);
    let ttl_hours = std::env::var("SUGGESTION_TTL_HOURS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(24);
    let suggestion_cache = Arc::new(
        SuggestionCache::from_env(Duration::from_secs(ttl_hours * 3600))?
    );

    tracing::info!("AI service starting up");
    tracing::info!("Gemini provider initialized");
    tracing::info!("Context aggregator initialized");
    tracing::info!("Workspace client initialized");
    tracing::info!("User config client initialized");
    tracing::info!("Suggestion cache initialized (Redis)");

    api::server::serve(
        orchestrator,
        aggregator,
        user_config_client,
        suggestion_cache,
        workspace_client,
        provider,
    ).await?;

    Ok(())
}
