// api/server.rs

use std::sync::Arc;
use tonic::transport::Server;
use crate::api::proto::ai_service_server::AiServiceServer;
use crate::api::handlers::grading_handler::GradingHandler;
use crate::orchestration::orchestrator::Orchestrator;
use crate::context::aggregator::ContextAggregator;
use crate::context::suggestion_cache::SuggestionCache;
use crate::context::user_config_client::UserConfigClient;
use crate::context::workspace_client::WorkspaceClient;
use crate::domain::ports::llm_provider::LlmProvider;

pub async fn serve(
    orchestrator: Arc<Orchestrator>,
    aggregator: Arc<ContextAggregator>,
    user_config_client: Arc<UserConfigClient>,
    suggestion_cache: Arc<SuggestionCache>,
    workspace_client: Arc<WorkspaceClient>,
    provider: Arc<dyn LlmProvider>,
) -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("GRPC_PORT").unwrap_or_else(|_| "50051".to_string());
    let addr = format!("0.0.0.0:{}", port).parse()?;

    let handler = GradingHandler::new(
        orchestrator,
        aggregator,
        user_config_client,
        suggestion_cache,
        workspace_client,
        provider,
    );
    let service = AiServiceServer::new(handler);

    tracing::info!("gRPC server listening on {}", addr);

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await?;

    Ok(())
}
