// api/server.rs

use std::sync::Arc;
use tonic::transport::Server;
use crate::api::proto::ai_service_server::AiServiceServer;
use crate::api::handlers::grading_handler::GradingHandler;
use crate::orchestration::orchestrator::Orchestrator;

/// Builds and runs the gRPC server.
///
/// Binds to the address in the `GRPC_PORT` environment variable,
/// defaulting to 50051 if not set.
/// Blocks until the server shuts down.
pub async fn serve(orchestrator: Arc<Orchestrator>) -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("GRPC_PORT").unwrap_or_else(|_| "50051".to_string());
    let addr = format!("0.0.0.0:{}", port).parse()?;

    let handler = GradingHandler::new(orchestrator);
    let service = AiServiceServer::new(handler);

    tracing::info!("gRPC server listening on {}", addr);

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await?;

    Ok(())
}