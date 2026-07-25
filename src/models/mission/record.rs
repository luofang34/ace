//! Mission result assembly and diagnostic scoping.

use crate::domain::diagnostic::Diagnostic;
use crate::domain::result::{MissionSegmentResult, ModelMetadata};
use crate::domain::schema::MissionSegment;

use super::{MissionState, SegmentComputation};

pub(super) fn mission_model(completed: bool) -> ModelMetadata {
    ModelMetadata {
        model_id: "mission.quasi_steady".to_owned(),
        model_version: "1.0.0".to_owned(),
        fidelity_level: 1,
        validity_status: if completed { "valid" } else { "incomplete" }.to_owned(),
    }
}

pub(super) fn segment_result(
    segment: &MissionSegment,
    state: MissionState,
    computation: &SegmentComputation,
    warnings: Vec<Diagnostic>,
) -> MissionSegmentResult {
    MissionSegmentResult {
        segment_id: segment.id.clone(),
        start_mass_kg: state.mass_kg,
        end_mass_kg: state.mass_kg - computation.fuel_burn_kg - computation.payload_removed_kg,
        fuel_burn_kg: computation.fuel_burn_kg,
        payload_removed_kg: computation.payload_removed_kg,
        distance_m: computation.distance_m,
        duration_s: computation.duration_s,
        start_altitude_m: state.altitude_m,
        end_altitude_m: computation.end_altitude_m,
        operating_speed_m_s: computation.operating_speed_m_s,
        warnings,
    }
}

pub(super) fn extend_unique_diagnostics(
    target: &mut Vec<Diagnostic>,
    diagnostics: Vec<Diagnostic>,
) {
    for diagnostic in diagnostics {
        if !target
            .iter()
            .any(|existing| existing.code == diagnostic.code && existing.path == diagnostic.path)
        {
            target.push(diagnostic);
        }
    }
}

pub(super) fn scoped_segment_warnings(
    segment: &MissionSegment,
    warnings: &[Diagnostic],
) -> Vec<Diagnostic> {
    warnings
        .iter()
        .cloned()
        .map(|mut warning| {
            let base = format!("mission.segments.{}", segment.id);
            warning.path = warning
                .path
                .as_deref()
                .map_or_else(|| Some(base.clone()), |path| Some(format!("{base}.{path}")));
            warning
        })
        .collect()
}
