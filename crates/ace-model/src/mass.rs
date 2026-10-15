use crate::{Binding, Source};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Contribution to a vehicle's mass statement.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MassKind {
    /// Operating empty mass.
    Empty,
    /// Variable carried payload.
    Payload,
    /// Consumable fuel.
    Fuel,
    /// Other explicitly classified component.
    Other,
}

/// Symmetric tensor about the item's own CG in vehicle axes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Inertia {
    /// Ixx.
    pub xx: Binding,
    /// Iyy.
    pub yy: Binding,
    /// Izz.
    pub zz: Binding,
    /// Ixy tensor component.
    pub xy: Binding,
    /// Ixz tensor component.
    pub xz: Binding,
    /// Iyz tensor component.
    pub yz: Binding,
}

/// Center of mass at a declared fraction of usable capacity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FillStation {
    /// Fraction between zero and one.
    pub fraction: f64,
    /// X/y/z coordinates.
    pub cg: [Binding; 3],
}

/// Non-overlapping component with a three-dimensional mass distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MassItem {
    /// Contribution category.
    pub kind: MassKind,
    /// Current loaded mass.
    pub mass: Binding,
    /// Center of mass in `frame`, or vehicle coordinates when no frame is declared.
    pub cg: [Binding; 3],
    /// Geometry-local frame shared by CG, fill stations and intrinsic tensor axes.
    pub frame: Option<String>,
    /// Tensor about the component's own CG at the declared mass; absence is unknown.
    /// Loading overrides require a new tensor from a solver or remain unknown.
    pub inertia: Option<Inertia>,
    /// Semantic geometry component.
    pub geometry: Option<String>,
    /// Capacity for a variable fuel/payload item.
    pub capacity: Option<Binding>,
    /// Ordered fill-dependent CG interpolation stations.
    #[serde(default)]
    pub fill_curve: Vec<FillStation>,
    /// Mass estimation or measurement provenance.
    pub source: Source,
}

/// Named loading condition with per-item mass overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LoadingCase {
    /// Overrides keyed by mass item identifier.
    pub masses: BTreeMap<String, Binding>,
}
