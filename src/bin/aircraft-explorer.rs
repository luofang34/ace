//! Command-line entry point for Aircraft Concept Explorer.

use std::process::ExitCode;

fn main() -> ExitCode {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(aircraft_concept_explorer::run_cli()),
        Err(source) => {
            tracing::error!("failed to initialize runtime: {source}");
            ExitCode::FAILURE
        }
    }
}
