use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawGeometry {
    pub(crate) wing: RawWing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fuselage: Option<RawFuselageGeometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) horizontal_tail: Option<RawTailGeometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) vertical_tail: Option<RawTailGeometry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawWing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) area: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) span: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) aspect_ratio: Option<f64>,
    pub(crate) sweep_quarter_chord: String,
    #[serde(default)]
    pub(crate) center_body_edge_sweep: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(crate) struct RawFuselageGeometry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) length: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) diameter: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(crate) struct RawTailGeometry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) area: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) arm: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AircraftGeometry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fuselage: Option<FuselageGeometry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) horizontal_tail: Option<TailGeometry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) vertical_tail: Option<TailGeometry>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FuselageGeometry {
    pub(crate) length: GeometryValue,
    pub(crate) diameter: GeometryValue,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TailGeometry {
    pub(crate) area: GeometryValue,
    pub(crate) arm: GeometryValue,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GeometryValue {
    pub(crate) value: f64,
    pub(crate) unit: &'static str,
    pub(crate) provenance: GeometryValueProvenance,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GeometryValueProvenance {
    pub(crate) kind: &'static str,
    pub(crate) source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) correlation_id: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) correlation_version: Option<u32>,
    pub(crate) explicitly_provided: bool,
}
