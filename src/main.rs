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
use std::time::Duration;

use dotenvy;
use integration::gemini::client::GeminiClient;
use integration::openai::OpenAiClient;
use orchestration::orchestrator::Orchestrator;
use context::aggregator::ContextAggregator;
use context::suggestion_cache::SuggestionCache;
use context::user_config_client::UserConfigClient;
use context::workspace_client::WorkspaceClient;
use application::workflows::grading::grading_workflow::WorkflowContext;

fn select_provider() -> Result<Arc<dyn domain::ports::llm_provider::LlmProvider>, Box<dyn std::error::Error>> {
    let provider_kind = std::env::var("LLM_PROVIDER")
        .unwrap_or_else(|_| "gemini".to_string());

    match provider_kind.to_lowercase().as_str() {
        "openai" | "groq" => {
            let client = OpenAiClient::from_env()?;
            tracing::info!("OpenAI-compatible provider initialized (model={})", client.model);
            Ok(Arc::new(client))
        }
        _ => {
            let client = GeminiClient::from_env()?;
            tracing::info!("Gemini provider initialized (model={})", client.model);
            Ok(Arc::new(client))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let provider = select_provider()?;
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

    let workflow_ctx = Arc::new(WorkflowContext {
        provider: provider.clone(),
        aggregator: aggregator.clone(),
        workspace_client: workspace_client.clone(),
    });
    let orchestrator = Arc::new(Orchestrator::new(workflow_ctx));

    tracing::info!("AI service starting up");
    tracing::info!("Context aggregator initialized");
    tracing::info!("Workspace client initialized");
    tracing::info!("User config client initialized");
    tracing::info!("Suggestion cache initialized (Redis)");

    api::server::serve(
        orchestrator,
        user_config_client,
        suggestion_cache,
        workspace_client,
        provider,
    ).await?;

    Ok(())
}
