//! Short command alias for Aircraft Concept Explorer.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    aircraft_concept_explorer::run_cli().await
}
