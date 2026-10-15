use std::env;
use std::ffi::OsString;
use std::process::ExitCode;

mod arguments;
pub(crate) mod commands;
mod failure;
mod grammar;
mod output;
mod parsing;
mod plots;
mod vehicle;

use arguments::*;

/// Run the Aircraft Concept Explorer command-line interface.
pub async fn run_cli() -> ExitCode {
    drop(
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
            )
            .with_writer(std::io::stderr)
            .try_init(),
    );
    let arguments: Vec<OsString> = env::args_os().collect();
    let json = failure::json_requested(&arguments);
    let cli = match grammar::command()
        .try_get_matches_from(&arguments)
        .and_then(parsing::command)
    {
        Ok(cli) => cli,
        Err(error) if is_help(&error) => {
            return match error.print() {
                Ok(()) => ExitCode::SUCCESS,
                Err(source) => {
                    tracing::error!("failed to write CLI help: {source}");
                    ExitCode::FAILURE
                }
            };
        }
        Err(error) => {
            present_error(&failure::clap_detail(&error), &error.to_string(), json);
            return ExitCode::FAILURE;
        }
    };
    match commands::execute(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            present_error(&error.detail(), &error.to_string(), json);
            ExitCode::FAILURE
        }
    }
}

fn is_help(error: &clap::Error) -> bool {
    matches!(
        error.kind(),
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
    )
}

fn present_error(detail: &crate::domain::diagnostic::ErrorDetail, human_message: &str, json: bool) {
    if json {
        if let Err(source) = failure::emit_json(detail) {
            tracing::error!("failed to write structured error: {source}");
        }
    } else {
        tracing::error!("{human_message}");
    }
}
