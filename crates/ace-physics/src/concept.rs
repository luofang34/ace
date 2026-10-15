use crate::{PhysicsError, PhysicsResult, geometry::frames};
use ace_model::{AnalysisResources, GeometryRole, Quantity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod mesh;
mod section;

/// Closed tessellation in vehicle coordinates, measured in metres.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfacePart {
    /// Vertex coordinates.
    pub vertices: Vec<[f64; 3]>,
    /// Triangle indices into vertices.
    pub triangles: Vec<[usize; 3]>,
}

/// A semantic conceptual object, with independently convex surface parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptObject {
    /// Canonical geometry identifier.
    pub id: String,
    /// Semantic role.
    pub role: GeometryRole,
    /// Tessellated surfaces, preserving nonconvex unions such as swept wings.
    pub parts: Vec<SurfacePart>,
    /// Vehicle-coordinate extents.
    pub minimum_m: [f64; 3],
    /// Vehicle-coordinate extents.
    pub maximum_m: [f64; 3],
}

/// Reproducible native geometric product, not a structural or aerodynamic certification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptGeometry {
    /// Surface construction and fidelity declaration.
    pub method: String,
    /// Generated objects; absent shapes are never invented by the drawing layer.
    pub objects: BTreeMap<String, ConceptObject>,
    /// Sum of triangulated part areas, including interpenetrating and root surfaces.
    pub part_area_m2: f64,
}

/// Evaluate model-bound body and lifting-surface shapes in their shared coordinate frames.
pub fn evaluate_concept_geometry(
    resources: &AnalysisResources,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<ConceptGeometry> {
    let mut objects = BTreeMap::new();
    let mut part_area_m2 = 0.0;
    for (id, object) in &resources.geometry {
        let Some(shape) = &object.shape else { continue };
        let transform = frames::resolve(id, resources, values)?;
        let mut parts = mesh::generate(shape, values)?;
        let mut minimum_m = [f64::INFINITY; 3];
        let mut maximum_m = [f64::NEG_INFINITY; 3];
        for part in &mut parts {
            for point in &mut part.vertices {
                *point = transform.point(*point)?;
                for axis in 0..3 {
                    minimum_m[axis] = minimum_m[axis].min(point[axis]);
                    maximum_m[axis] = maximum_m[axis].max(point[axis]);
                }
            }
            part_area_m2 += mesh::area(part)?;
        }
        objects.insert(
            id.clone(),
            ConceptObject {
                id: id.clone(),
                role: object.role.clone(),
                parts,
                minimum_m,
                maximum_m,
            },
        );
    }
    if objects.is_empty() {
        return Err(PhysicsError::invalid(
            "concept surfaces",
            "no declared native shapes",
        ));
    }
    Ok(ConceptGeometry {
        method: "ellipsoidal body ends; trapezoidal diamond-section wings; tessellated conceptual geometry; no structure or airfoil validation".into(),
        objects, part_area_m2,
    })
}

/// Intersect the actual tessellation at an x station, returning vehicle y/z line segments.
pub fn concept_section(object: &ConceptObject, station_m: f64) -> Vec<[[f64; 2]; 2]> {
    section::intersect(object, station_m)
}

/// Area of a closed section of one convex body part; overlapping objects are not summed.
pub fn concept_section_area(object: &ConceptObject, station_m: f64) -> f64 {
    section::area(object, station_m)
}

#[cfg(test)]
mod tests;
