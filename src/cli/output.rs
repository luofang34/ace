use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;

use crate::cli::{OutputArgs, OutputFormat};
use crate::domain::diagnostic::{AexError, AexResult};

pub(super) fn emit_blocking<T: Serialize>(value: &T, output: &OutputArgs) -> AexResult<()> {
    let raw = serde_json::to_value(value).map_err(|source| AexError::Json { source })?;
    let interface_value = crate::mcp::serialization::attach_units(raw);
    let bytes = serialize(&interface_value, output.format)?;
    match &output.output {
        Some(path) => fs::write(path, bytes).map_err(|source| AexError::Write {
            path: path.clone(),
            source,
        }),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(&bytes).map_err(|source| AexError::Write {
                path: Path::new("<stdout>").to_path_buf(),
                source,
            })?;
            handle.write_all(b"\n").map_err(|source| AexError::Write {
                path: Path::new("<stdout>").to_path_buf(),
                source,
            })
        }
    }
}

fn serialize<T: Serialize>(value: &T, format: OutputFormat) -> AexResult<Vec<u8>> {
    match format {
        OutputFormat::Json => {
            serde_json::to_vec_pretty(value).map_err(|source| AexError::Json { source })
        }
        OutputFormat::Yaml | OutputFormat::Table => serde_yaml::to_string(value)
            .map(String::into_bytes)
            .map_err(|source| AexError::Yaml {
                path: Path::new("<output>").to_path_buf(),
                source,
            }),
        OutputFormat::Csv => {
            let json = serde_json::to_string(value).map_err(|source| AexError::Json { source })?;
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["result"]).map_err(csv_output_error)?;
            writer.write_record([json]).map_err(csv_output_error)?;
            writer.into_inner().map_err(|source| AexError::Write {
                path: Path::new("<output>").to_path_buf(),
                source: source.into_error(),
            })
        }
    }
}

fn csv_output_error(source: csv::Error) -> AexError {
    AexError::Write {
        path: Path::new("<output>").to_path_buf(),
        source: io::Error::other(source),
    }
}
