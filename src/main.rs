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
use context::llm_cache::LlmCache;
use context::suggestion_cache::SuggestionCache;
use context::user_config_client::UserConfigClient;
use context::vector_store::{VectorStore, VectorStoreHandle};
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

fn init_vector_store() -> Option<VectorStoreHandle> {
    let rag_enabled = std::env::var("RAG_ENABLED")
        .ok()
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(true);

    if !rag_enabled {
        tracing::info!("RAG is disabled — skipping Qdrant initialization");
        return None;
    }

    match VectorStore::from_env() {
        Ok(store) => {
            let handle = Arc::new(store);
            let init_handle = handle.clone();
            tokio::spawn(async move {
                match init_handle.ensure_collection().await {
                    Ok(()) => tracing::info!("Qdrant collection ready"),
                    Err(e) => tracing::warn!(error = %e, "failed to initialize Qdrant collection — RAG will be degraded"),
                }
            });
            tracing::info!("Vector store initialized (Qdrant)");
            Some(handle)
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to initialize vector store — RAG will be unavailable");
            None
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
    let vector_store = init_vector_store();

    let llm_cache_ttl_hours = std::env::var("LLM_CACHE_TTL_HOURS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(24);
    let llm_cache = match LlmCache::from_env(Duration::from_secs(llm_cache_ttl_hours * 3600)) {
        Ok(cache) => {
            tracing::info!("LLM response cache initialized (Redis, TTL={}h)", llm_cache_ttl_hours);
            Some(Arc::new(cache))
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to initialize LLM cache — proceeding without caching");
            None
        }
    };

    let workflow_ctx = Arc::new(WorkflowContext {
        provider: provider.clone(),
        aggregator: aggregator.clone(),
        workspace_client: workspace_client.clone(),
        vector_store,
        llm_cache,
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
