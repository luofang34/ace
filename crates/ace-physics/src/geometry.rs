use crate::bindings::{nonnegative, scalar};
use crate::{PhysicsError, PhysicsResult};
use ace_model::{AnalysisResources, GeometryRole, Quantity, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod envelopes;
pub(crate) mod frames;
mod sections;

/// Resolved model-space envelope; measurements describe the envelope fidelity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// Semantic role.
    pub role: GeometryRole,
    /// Minimum x/y/z coordinates in metres.
    pub minimum_m: [f64; 3],
    /// Maximum x/y/z coordinates in metres.
    pub maximum_m: [f64; 3],
    /// Volume of the bounding box, not usable/internal volume.
    pub envelope_volume_m3: f64,
    /// Explicit usable volume if supplied.
    pub usable_volume_m3: Option<f64>,
    /// Explicit wetted area if supplied.
    pub wetted_area_m2: Option<f64>,
}

/// Conservative pairwise clearance with closest-point witnesses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearanceResult {
    /// First semantic object.
    pub first: String,
    /// Second semantic object.
    pub second: String,
    /// Signed distance, negative for overlapping envelopes.
    pub clearance_m: f64,
    /// Required separation.
    pub required_m: f64,
    /// Signed constraint slack.
    pub margin_m: f64,
    /// Closest point on the first envelope.
    pub first_point_m: [f64; 3],
    /// Closest point on the second envelope.
    pub second_point_m: [f64; 3],
}

/// A rectangular envelope intersection in a selected section plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionSlice {
    /// Semantic object being intersected.
    pub geometry: String,
    /// Lower in-plane coordinates, metres.
    pub minimum_m: [f64; 2],
    /// Upper in-plane coordinates, metres.
    pub maximum_m: [f64; 2],
}

/// Critical or requested cross-section with its engineering reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionResult {
    /// Stable section identifier.
    pub id: String,
    /// Plane-normal axis, x=0, y=1, z=2.
    pub axis: u8,
    /// Plane station in metres.
    pub station_m: f64,
    /// Why the section is included.
    pub reason: String,
    /// Related clearance check, when generated from an active clearance.
    pub clearance: Option<String>,
    /// Actual intersected envelope rectangles.
    pub slices: Vec<SectionSlice>,
}

/// Area of the union of envelope sections at a longitudinal station.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaStation {
    /// Longitudinal coordinate in metres.
    pub station_m: f64,
    /// Union area in square metres.
    pub envelope_area_m2: f64,
}

/// Geometry projection usable by the synthesis sheet and downstream constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeometryReport {
    /// Explicit fidelity and scope of the geometric calculations.
    pub method: String,
    /// Resolved geometry envelopes.
    pub envelopes: BTreeMap<String, Envelope>,
    /// Pairwise clearance evidence.
    pub clearances: BTreeMap<String, ClearanceResult>,
    /// Automatically ranked and requested critical sections.
    pub sections: Vec<SectionResult>,
    /// Longitudinal envelope area distribution.
    pub area_distribution: Vec<AreaStation>,
}

/// Rigid local-to-vehicle placement of one selected object, composing its parent frames.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Frame {
    /// Proper orthonormal rotation mapping local vectors to vehicle axes.
    pub rotation: [[f64; 3]; 3],
    /// Local origin in vehicle coordinates, metres.
    pub translation: [f64; 3],
}

/// Resolve the vehicle-frame placement of a selected geometry object or frame reference.
pub fn resolve_frame(
    id: &str,
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<Frame> {
    let transform = frames::resolve(id, resources, values)?;
    Ok(Frame {
        rotation: transform.rotation,
        translation: transform.translation,
    })
}

/// Resolve declared geometry and evaluate conservative envelope clearances.
pub fn evaluate_geometry(
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<GeometryReport> {
    let envelopes = envelopes::resolve(resources, values)?;
    let mut clearances = BTreeMap::new();
    for (id, request) in &resources.clearances {
        let first = envelopes
            .get(&request.first)
            .ok_or_else(|| PhysicsError::invalid(id, "first envelope unavailable"))?;
        let second = envelopes
            .get(&request.second)
            .ok_or_else(|| PhysicsError::invalid(id, "second envelope unavailable"))?;
        let required = nonnegative(id, scalar(&request.minimum, values, Unit::Meter)?)?;
        clearances.insert(
            id.clone(),
            separation(&request.first, &request.second, first, second, required),
        );
    }
    let section_results = sections::critical(resources, &envelopes, &clearances, values)?;
    let area_distribution = sections::area_distribution(&envelopes);
    Ok(GeometryReport {
        method: "axis_aligned_envelopes".into(),
        envelopes,
        clearances,
        sections: section_results,
        area_distribution,
    })
}

fn separation(
    first: &str,
    second: &str,
    a: &Envelope,
    b: &Envelope,
    required: f64,
) -> ClearanceResult {
    let mut pa = [0.0; 3];
    let mut pb = [0.0; 3];
    let mut penetration = f64::INFINITY;
    for axis in 0..3 {
        if a.maximum_m[axis] < b.minimum_m[axis] {
            pa[axis] = a.maximum_m[axis];
            pb[axis] = b.minimum_m[axis];
        } else if b.maximum_m[axis] < a.minimum_m[axis] {
            pa[axis] = a.minimum_m[axis];
            pb[axis] = b.maximum_m[axis];
        } else {
            let low = a.minimum_m[axis].max(b.minimum_m[axis]);
            let high = a.maximum_m[axis].min(b.maximum_m[axis]);
            pa[axis] = (low + high) * 0.5;
            pb[axis] = pa[axis];
        }
        penetration = penetration.min(
            (a.maximum_m[axis] - b.minimum_m[axis]).min(b.maximum_m[axis] - a.minimum_m[axis]),
        );
    }
    let distance = pa
        .iter()
        .zip(pb)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt();
    let clearance_m = if distance == 0.0 {
        -penetration.max(0.0)
    } else {
        distance
    };
    ClearanceResult {
        first: first.into(),
        second: second.into(),
        clearance_m,
        required_m: required,
        margin_m: clearance_m - required,
        first_point_m: pa,
        second_point_m: pb,
    }
}

#[cfg(test)]
mod tests;
