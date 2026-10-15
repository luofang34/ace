use super::AexMcpServer;
use crate::domain::diagnostic::{AexError, AexResult};
use crate::services::analysis::ApplicationService;
use rmcp::{ServiceExt, transport::stdio};

pub(crate) async fn serve_stdio(service: ApplicationService) -> AexResult<()> {
    let server = AexMcpServer::new(service)
        .serve(stdio())
        .await
        .map_err(|source| AexError::analysis("MCP_SERVE_ERROR", source.to_string()))?;
    server
        .waiting()
        .await
        .map_err(|source| AexError::analysis("MCP_TRANSPORT_ERROR", source.to_string()))?;
    Ok(())
}
