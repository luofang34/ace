use std::ffi::OsString;
use std::io::{self, Write};

use clap::error::ErrorKind;
use serde::Serialize;

use crate::domain::diagnostic::ErrorDetail;

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: &'a ErrorDetail,
}

pub(super) fn json_requested(arguments: &[OsString]) -> bool {
    arguments.iter().enumerate().any(|(index, argument)| {
        argument
            .to_str()
            .is_some_and(|value| value == "--format=json")
            || (argument == "--format"
                && arguments
                    .get(index.wrapping_add(1))
                    .is_some_and(|value| value == "json"))
    })
}

pub(super) fn clap_detail(error: &clap::Error) -> ErrorDetail {
    ErrorDetail {
        code: clap_code(error.kind()).to_owned(),
        message: error.to_string().trim().to_owned(),
        path: Some("arguments".to_owned()),
        context: serde_json::json!({
            "kind": format!("{:?}", error.kind()),
        }),
    }
}

pub(super) fn emit_json(detail: &ErrorDetail) -> io::Result<()> {
    let bytes = serde_json::to_vec(&ErrorEnvelope { error: detail }).map_err(io::Error::other)?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(&bytes)?;
    handle.write_all(b"\n")
}

fn clap_code(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::ArgumentConflict => "CLI_ARGUMENT_CONFLICT",
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => "CLI_INVALID_VALUE",
        ErrorKind::InvalidSubcommand => "CLI_INVALID_SUBCOMMAND",
        ErrorKind::NoEquals => "CLI_MISSING_EQUALS",
        ErrorKind::TooFewValues | ErrorKind::TooManyValues | ErrorKind::WrongNumberOfValues => {
            "CLI_INVALID_VALUE_COUNT"
        }
        ErrorKind::UnknownArgument => "CLI_UNKNOWN_ARGUMENT",
        ErrorKind::MissingRequiredArgument | ErrorKind::MissingSubcommand => "CLI_MISSING_ARGUMENT",
        _ => "CLI_PARSE_ERROR",
    }
}

#[cfg(test)]
mod tests;
