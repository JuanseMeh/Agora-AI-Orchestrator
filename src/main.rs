#![allow(dead_code)]
#![allow(unused_imports)]

mod api;
mod application;
mod domain;
mod integration;
mod models;
mod orchestration;

use std::sync::Arc;
use integration::gemini::client::GeminiClient;
use orchestration::orchestrator::Orchestrator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let provider = GeminiClient::from_env()?;
    let orchestrator = Arc::new(Orchestrator::new(Arc::new(provider)));

    api::server::serve(orchestrator).await?;

    Ok(())
}