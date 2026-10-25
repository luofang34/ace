use super::{
    arguments::{Command, CommonArgs, DrawingArgs},
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
            branding: args.branding,
            drawing_data: args.drawing_data,
            from_snapshot: args.from_snapshot,
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

/// Probabilistic, measured-data and fleet-level evaluations of synthesis models.
pub(super) fn system_blocking(command: Command) -> AexResult<()> {
    use crate::services::system;
    match command {
        Command::Uncertainty {
            scenario,
            plan,
            common,
        } => emit_canonical_blocking(
            &system::uncertainty_blocking(&scenario, &plan, &common.override_map())?,
            &common.output,
        ),
        Command::Calibrate {
            scenario,
            case,
            common,
        } => emit_canonical_blocking(
            &system::calibrate_blocking(&scenario, &case, &common.override_map())?,
            &common.output,
        ),
        Command::Campaign { campaign, output } => {
            emit_canonical_blocking(&system::campaign_blocking(&campaign)?, &output)
        }
        _ => Err(AexError::validation(
            "UNSUPPORTED_COMMAND",
            "command",
            "not a system evaluation command",
        )),
    }
}

/// Apply an edit file and `--set` assignments to a vehicle as one revision.
pub(super) fn edit_blocking(command: Command) -> AexResult<()> {
    let Command::Edit {
        scenario,
        edits,
        expected_revision,
        save_as,
        common,
    } = command
    else {
        return Err(AexError::validation(
            "UNSUPPORTED_COMMAND",
            "command",
            "not a vehicle edit command",
        ));
    };
    let edits: Vec<ace_model::ModelEdit> = match &edits {
        Some(path) => {
            let content = std::fs::read_to_string(path).map_err(|source| AexError::Read {
                path: path.clone(),
                source,
            })?;
            serde_yaml::from_str(&content).map_err(|source| AexError::Yaml {
                path: path.clone(),
                source,
            })?
        }
        None => Vec::new(),
    };
    let updates = common.override_map();
    let outcome = crate::services::vehicle_edit::edit_blocking(
        &scenario,
        &crate::services::vehicle_edit::VehicleEdit {
            edits: &edits,
            updates: &updates,
            expected_revision,
            save_as: save_as.as_deref(),
        },
    )?;
    emit_canonical_blocking(&outcome, &common.output)
}
