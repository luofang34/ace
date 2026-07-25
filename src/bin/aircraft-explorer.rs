//! Aircraft Concept Explorer CLI and MCP server process.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    aircraft_concept_explorer::run_cli().await
}
