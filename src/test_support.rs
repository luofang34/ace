use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::diagnostic::AexResult;
use crate::domain::schema::ResolvedScenario;
use crate::domain::topology::AircraftTopology;
use crate::services::resolver::ScenarioResolver;
use crate::storage::profile_store::FileProfileStore;

pub(crate) fn example_scenario(name: &str) -> AexResult<ResolvedScenario> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resolver = ScenarioResolver::new(Arc::new(FileProfileStore));
    resolver.resolve_blocking(
        &root.join("examples").join(name).join("scenario.yaml"),
        &BTreeMap::new(),
    )
}

pub(crate) fn fuel_exhaustion_scenario() -> AexResult<ResolvedScenario> {
    let mut scenario = example_scenario("x15")?;
    let mut segment = scenario.mission.segments.first().cloned().ok_or_else(|| {
        crate::domain::diagnostic::AexError::validation(
            "MISSING_TEST_SEGMENT",
            "mission.segments",
            "X-15 fixture requires a timed segment",
        )
    })?;
    segment.id = "depletion_probe".to_owned();
    segment.duration_s = Some(3_600.0);
    segment.power_fraction = None;
    segment.thrust_fraction = Some(1.0);
    scenario.mission.segments = vec![segment];
    Ok(scenario)
}

pub(crate) fn set_inferred_configuration(
    scenario: &mut ResolvedScenario,
    configuration: &str,
) -> AexResult<()> {
    scenario.aircraft.configuration = configuration.to_owned();
    scenario.aircraft.topology = AircraftTopology::resolve(
        None,
        configuration,
        scenario.aircraft.propulsion.engine_count,
        scenario.aircraft.propulsion.propeller_profile.is_some(),
    )?;
    Ok(())
}
