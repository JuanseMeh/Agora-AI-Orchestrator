use std::sync::Arc;
use tonic::transport::Server;
use crate::api::proto::ai_service_server::AiServiceServer;
use crate::api::handlers::grading_handler::GradingHandler;
use crate::context::llm_cache::LlmCacheHandle;
use crate::context::suggestion_cache::SuggestionCache;
use crate::context::user_config_client::UserConfigClient;
use crate::context::vector_store::VectorStoreHandle;
use crate::context::workspace_client::WorkspaceClient;
use crate::domain::ports::llm_provider::LlmProvider;
use crate::orchestration::orchestrator::Orchestrator;

pub async fn serve(
    orchestrator: Arc<Orchestrator>,
    user_config_client: Arc<UserConfigClient>,
    suggestion_cache: Arc<SuggestionCache>,
    workspace_client: Arc<WorkspaceClient>,
    provider: Arc<dyn LlmProvider>,
    vector_store: Option<VectorStoreHandle>,
    llm_cache: Option<LlmCacheHandle>,
) -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("GRPC_PORT").unwrap_or_else(|_| "50051".to_string());
    let addr = format!("0.0.0.0:{}", port).parse()?;

    let handler = GradingHandler::new(
        orchestrator,
        user_config_client,
        suggestion_cache,
        workspace_client,
        provider,
        vector_store,
        llm_cache,
    );
    let service = AiServiceServer::new(handler);

    tracing::info!("gRPC server listening on {}", addr);

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await?;

    Ok(())
}
