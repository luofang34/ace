use super::{Envelope, frames};
use crate::{
    PhysicsError, PhysicsResult,
    bindings::{nonnegative, scalar, vector},
};
use ace_model::{AnalysisResources, Quantity, Unit};
use std::collections::BTreeMap;

pub(super) fn resolve(
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<BTreeMap<String, Envelope>> {
    let mut envelopes = BTreeMap::new();
    let concept = if resources
        .geometry
        .values()
        .any(|object| object.shape.is_some())
    {
        Some(crate::evaluate_concept_geometry(resources, values)?)
    } else {
        None
    };
    for (id, object) in &resources.geometry {
        let (minimum_m, maximum_m) =
            if let Some(shape) = concept.as_ref().and_then(|c| c.objects.get(id)) {
                (shape.minimum_m, shape.maximum_m)
            } else {
                let bounds = object
                    .bounds
                    .as_ref()
                    .ok_or_else(|| PhysicsError::invalid(id, "envelope solver requires bounds"))?;
                let minimum_m = vector(&bounds.minimum, values)?;
                let maximum_m = vector(&bounds.maximum, values)?;
                for (lower, upper) in minimum_m.iter().zip(maximum_m) {
                    if upper <= *lower {
                        return Err(PhysicsError::invalid(
                            id,
                            "local bounds require positive extents",
                        ));
                    }
                }
                let transform = frames::resolve(id, resources, values)?;
                let (minimum_m, maximum_m) = frames::bounds(&transform, minimum_m, maximum_m)?;
                (minimum_m, maximum_m)
            };
        let mut volume = 1.0;
        for (lower, upper) in minimum_m.iter().zip(maximum_m) {
            if upper <= *lower {
                return Err(PhysicsError::invalid(
                    id,
                    "bounds must have positive extent on every axis",
                ));
            }
            volume *= upper - lower;
        }
        let usable_volume_m3 = object
            .volume
            .as_ref()
            .map(|value| scalar(value, values, Unit::CubicMeter))
            .transpose()?;
        let wetted_area_m2 = object
            .wetted_area
            .as_ref()
            .map(|value| scalar(value, values, Unit::SquareMeter))
            .transpose()?;
        for value in [usable_volume_m3, wetted_area_m2].into_iter().flatten() {
            nonnegative(id, value)?;
        }
        nonnegative(id, volume)?;
        envelopes.insert(
            id.clone(),
            Envelope {
                role: object.role.clone(),
                minimum_m,
                maximum_m,
                envelope_volume_m3: volume,
                usable_volume_m3,
                wetted_area_m2,
            },
        );
    }
    Ok(envelopes)
}
