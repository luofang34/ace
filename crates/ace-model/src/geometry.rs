use crate::{Binding, Source};
use serde::{Deserialize, Serialize};

mod placement;
mod shape;
pub use placement::Placement;
pub use shape::ConceptShape;

/// Role understood by arrangement, mass, clearance, and capacity analyses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GeometryRole {
    /// Lifting surface.
    Wing,
    /// Usable fuel volume.
    FuelTank,
    /// Occupied cabin volume.
    Cabin,
    /// Cargo volume.
    CargoBay,
    /// Installed engine.
    Engine,
    /// Propulsion inlet.
    Inlet,
    /// Landing gear swept envelope.
    LandingGear,
    /// Pressurized shell.
    PressureVessel,
    /// Structural partition.
    Bulkhead,
    /// Electronics installation.
    AvionicsBay,
    /// Structural member.
    Structure,
    /// Domain extension without changing the vehicle schema.
    Other(String),
}

/// Conservative axis-aligned envelope in the vehicle's common reference frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    /// Minimum x/y/z coordinates.
    pub minimum: [Binding; 3],
    /// Maximum x/y/z coordinates.
    pub maximum: [Binding; 3],
}

/// Semantic geometry with versioned assets and parameter-bound physical extents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GeometryObject {
    /// Meaning of this component or reserved volume.
    pub role: GeometryRole,
    /// Human-readable component label.
    pub name: String,
    /// Source of the geometry definition.
    pub source: Source,
    /// Geometry definition revision.
    #[serde(default)]
    pub revision: u64,
    /// Optional containing semantic object.
    pub parent: Option<String>,
    /// Conservative extents in this object's local frame.
    pub bounds: Option<Bounds>,
    /// Shared placement for geometry and mass properties bound to this frame.
    #[serde(default)]
    pub placement: Placement,
    /// Optional native conceptual surface; its parameters remain authoritative model inputs.
    #[serde(default)]
    pub shape: Option<Box<ConceptShape>>,
    /// Usable physical volume.
    pub volume: Option<Binding>,
    /// Wetted surface area.
    pub wetted_area: Option<Binding>,
    /// Content-addressed CAD/mesh artifact references.
    #[serde(default)]
    pub assets: Vec<String>,
    /// Verified content identities for parameterized geometry and external CAD products.
    #[serde(default)]
    pub artifacts: Vec<crate::Artifact>,
}

impl GeometryObject {
    /// Physical fields whose parameter bindings must invalidate dependent analyses.
    pub fn bindings(&self) -> Vec<(&Binding, crate::Unit)> {
        let mut bindings = Vec::new();
        if let Some(shape) = &self.shape {
            bindings.extend(shape.bindings());
        }
        bindings.extend(
            self.placement
                .translation
                .iter()
                .map(|value| (value, crate::Unit::Meter)),
        );
        if let Some(bounds) = &self.bounds {
            bindings.extend(
                bounds
                    .minimum
                    .iter()
                    .chain(&bounds.maximum)
                    .map(|value| (value, crate::Unit::Meter)),
            );
        }
        if let Some(volume) = &self.volume {
            bindings.push((volume, crate::Unit::CubicMeter));
        }
        if let Some(area) = &self.wetted_area {
            bindings.push((area, crate::Unit::SquareMeter));
        }
        bindings
    }
}

/// Required separation of two distinct semantic objects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Clearance {
    /// First object identifier.
    pub first: String,
    /// Second object identifier.
    pub second: String,
    /// Required minimum separation.
    pub minimum: Binding,
}

/// Structural or packaging station eligible for a critical section view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Section {
    /// Axis index, x=0, y=1, z=2.
    pub axis: u8,
    /// Station along that axis.
    pub station: Binding,
    /// Objects to include in the section.
    pub geometry: Vec<String>,
    /// Engineering reason for this station.
    pub reason: String,
}
