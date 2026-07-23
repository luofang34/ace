//! Aircraft Concept Explorer CLI and MCP server process.

#[tokio::main]
async fn main() -> Result<(), aircraft_concept_explorer::AexError> {
    aircraft_concept_explorer::run_cli().await
}
