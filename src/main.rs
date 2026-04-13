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
use api::handlers::grading_handler::GradingHandler;
use dotenvy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

        dotenvy::dotenv().ok();

    let provider = GeminiClient::from_env()?;
    let orchestrator = Arc::new(Orchestrator::new(Arc::new(provider)));
    let aggregator = Arc::new(ContextAggregator::from_env()?);

    api::server::serve(orchestrator, aggregator).await?;

    Ok(())
}