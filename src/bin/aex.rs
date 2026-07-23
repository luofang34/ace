//! Short command alias for Aircraft Concept Explorer.

#[tokio::main]
async fn main() -> Result<(), aircraft_concept_explorer::AexError> {
    aircraft_concept_explorer::run_cli().await
}
