use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::diagnostic::AexResult;
use crate::domain::schema::ResolvedScenario;
use crate::services::analysis::ApplicationService;

pub(crate) fn example_scenario(name: &str) -> AexResult<ResolvedScenario> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let service = ApplicationService::filesystem(std::env::temp_dir().join("aex-unit-runs"));
    service.resolve_blocking(
        &root.join("examples").join(name).join("scenario.yaml"),
        &BTreeMap::new(),
    )
}
