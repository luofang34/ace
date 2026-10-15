use super::{
    arguments::{CommonArgs, DrawingArgs},
    output::emit_canonical_blocking,
};
use crate::domain::diagnostic::{AexError, AexResult};
use std::path::PathBuf;

pub(super) fn drawing_blocking(args: DrawingArgs) -> AexResult<()> {
    let result = crate::services::drawing::run_blocking(
        &args.source,
        &args.directory,
        &args.common.override_map(),
        crate::services::drawing::DrawingOptions {
            paper: args.paper,
            plan: args.plan,
            candidate: args.candidate,
            scenario: args.scenario,
            replay: args.replay,
        },
    )?;
    emit_canonical_blocking(&result, &args.common.output)
}

pub(super) async fn sheet(scenario: PathBuf, port: u16, common: CommonArgs) -> AexResult<()> {
    let server = crate::services::sheet::start(&scenario, port, &common.override_map()).await?;
    let result = async {
        emit_canonical_blocking(
            &serde_json::json!({"url":format!("http://{}",server.address),"model":scenario}),
            &common.output,
        )?;
        tokio::signal::ctrl_c()
            .await
            .map_err(|source| AexError::Service {
                operation: "wait for sheet shutdown",
                source: Box::new(source),
            })
    }
    .await;
    let shutdown = server.shutdown().await;
    result.and(shutdown)
}
