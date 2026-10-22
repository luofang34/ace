use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use ace_engine::{Margin, Registry, Session, Snapshot, evaluate_requirements};
use ace_model::{Quantity, Source, Unit, VehicleModel};
use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult};

mod flight;
mod geometry;
mod kernel;
mod vspaero;
mod weight;

#[derive(Debug, Serialize)]
pub(crate) struct SynthesisReport {
    model: VehicleModel,
    completed: bool,
    assessment: ace_engine::Assessment,
    snapshot: Snapshot,
    margins: BTreeMap<String, Margin>,
}

pub(crate) fn run_blocking(
    path: &Path,
    overrides: &BTreeMap<String, String>,
) -> AexResult<SynthesisReport> {
    let mut session = open_blocking(path, overrides)?;
    let snapshot = session.run()?;
    let margins = evaluate_requirements(session.model(), &snapshot)?;
    Ok(SynthesisReport {
        model: session.model().clone(),
        completed: snapshot.complete(),
        assessment: ace_engine::assess(session.model(), &snapshot)?,
        snapshot,
        margins,
    })
}

pub(crate) fn open_blocking(
    path: &Path,
    overrides: &BTreeMap<String, String>,
) -> AexResult<Session> {
    let content = fs::read_to_string(path).map_err(|source| AexError::Read {
        path: path.into(),
        source,
    })?;
    let model: VehicleModel = serde_yaml::from_str(&content).map_err(|source| AexError::Yaml {
        path: path.into(),
        source,
    })?;
    create_session_blocking(model, overrides)
}

pub(crate) fn create_session_blocking(
    model: VehicleModel,
    overrides: &BTreeMap<String, String>,
) -> AexResult<Session> {
    let mut registry = Registry::default();
    ace_physics::register(&mut registry)?;
    registry.register(Arc::new(weight::WeightPlugin::new()))?;
    for phase in flight_phases() {
        registry.register(Arc::new(phase))?;
    }
    geometry::register_blocking(&model, &mut registry)?;
    vspaero::register_blocking(&model, &mut registry)?;
    kernel::register_blocking(&model, &mut registry)?;
    let mut session = Session::new(model, registry)?;
    let updates = overrides
        .iter()
        .map(|(name, value)| Ok((name.clone(), parse_quantity(name, value)?)))
        .collect::<AexResult<BTreeMap<_, _>>>()?;
    session.edit(
        &updates,
        Source {
            kind: "user".into(),
            reference: "CLI override".into(),
            revision: None,
        },
    )?;
    Ok(session)
}

/// Every state-driven flight-phase adapter a synthesis session registers.
fn flight_phases() -> Vec<flight::FlightPhase> {
    use flight::{FlightPhase, PhaseKind};
    let mut phases = vec![FlightPhase::with_kind(PhaseKind::GroundEstimate)];
    for kind in [
        PhaseKind::Level,
        PhaseKind::RouteCruise,
        PhaseKind::EnergyPath,
    ] {
        phases.extend([
            FlightPhase::with_kind(kind),
            FlightPhase::with_vspaero(kind),
            FlightPhase::with_kind(kind).propeller(),
            FlightPhase::with_kind(kind).propeller().vspaero(),
        ]);
    }
    phases
}

/// Registered flight-phase adapter identifiers, as advertised by the capability manifest.
pub(crate) fn flight_phase_ids() -> Vec<String> {
    use ace_engine::AnalysisPlugin;
    flight_phases()
        .iter()
        .map(|phase| phase.spec().identity.id.clone())
        .collect()
}

fn parse_quantity(name: &str, raw: &str) -> AexResult<Quantity> {
    let (value, unit) = raw.split_once(char::is_whitespace).ok_or_else(|| {
        AexError::validation(
            "AMBIGUOUS_UNITLESS_VALUE",
            name,
            "use an explicit quantity such as 37 m",
        )
    })?;
    let value = value
        .parse::<f64>()
        .map_err(|source| AexError::validation("INVALID_QUANTITY", name, source.to_string()))?;
    let unit: Unit = serde_json::from_value(serde_json::Value::String(unit.trim().into()))
        .map_err(|source| {
            AexError::validation("INVALID_QUANTITY_UNIT", name, source.to_string())
        })?;
    Quantity::new(value, unit).map_err(|source| AexError::Synthesis {
        source: source.into(),
    })
}

#[cfg(test)]
#[path = "synthesis/transaction/tests.rs"]
mod transaction_tests;
