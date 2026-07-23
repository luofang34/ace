use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::domain::diagnostic::{AexError, AexResult};

pub(crate) fn read_yaml_blocking<T: DeserializeOwned>(path: &Path) -> AexResult<T> {
    let content = fs::read_to_string(path).map_err(|source| AexError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&content).map_err(|source| AexError::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn read_yaml_value_blocking(path: &Path) -> AexResult<serde_yaml::Value> {
    read_yaml_blocking(path)
}
