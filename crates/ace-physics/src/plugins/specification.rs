use super::Kind;
use ace_engine::{Fidelity, PluginIdentity, PluginSpec};
use ace_model::Unit;
use std::collections::BTreeMap;

pub(super) fn build(kind: Kind, revision: &str) -> PluginSpec {
    let id = match kind {
        Kind::ConceptGeometry => "vehicle.concept_geometry",
        Kind::CgEnvelope => "vehicle.mission_cg_envelope",
        Kind::FuelSizing => "vehicle.mission_fuel_requirement",
        Kind::Volume => "vehicle.usable_volume",
        Kind::WettedArea => "vehicle.wetted_area",
        Kind::Geometry => "vehicle.geometry_envelopes",
        Kind::Mass => "vehicle.mass_properties",
        Kind::Mission => "vehicle.mission_sequence",
        Kind::Operations => "vehicle.operations",
        Kind::TripCost => "vehicle.trip_cost",
        Kind::Phase => "vehicle.constant_rate_phase",
    };
    PluginSpec {
        identity: PluginIdentity {
            id: id.into(),
            version: "1".into(),
            fidelity: Fidelity::Conceptual,
            code_revision: revision.into(),
        },
        inputs: inputs(kind),
        outputs: outputs(kind),
    }
}

fn inputs(kind: Kind) -> BTreeMap<String, Unit> {
    use Unit::*;
    let ports: &[(&str, Unit)] = match kind {
        Kind::Phase => &[
            ("duration", Second),
            ("speed", MeterPerSecond),
            ("fuel_flow", KilogramPerSecond),
            ("cost_rate", DollarPerSecond),
        ],
        Kind::FuelSizing => &[("minimum_remaining", Kilogram), ("tolerance", Kilogram)],
        _ => &[],
    };
    ports
        .iter()
        .map(|(id, unit)| ((*id).into(), *unit))
        .collect()
}

fn outputs(kind: Kind) -> BTreeMap<String, Unit> {
    use Unit::*;
    let ports: &[(&str, Unit)] = match kind {
        Kind::ConceptGeometry => &[("part_area", SquareMeter)],
        Kind::CgEnvelope => &[("margin", Meter)],
        Kind::FuelSizing => &[("required_fuel", Kilogram)],
        Kind::Volume => &[("volume", CubicMeter)],
        Kind::WettedArea => &[("area", SquareMeter)],
        Kind::Geometry => &[("minimum_clearance", Meter), ("minimum_margin", Meter)],
        Kind::Mass => &[
            ("mass", Kilogram),
            ("cg_x", Meter),
            ("cg_y", Meter),
            ("cg_z", Meter),
            ("minimum_cg_x", Meter),
            ("maximum_cg_x", Meter),
        ],
        Kind::Mission => &[
            ("duration", Second),
            ("distance", Meter),
            ("block_fuel", Kilogram),
            ("reserve_fuel", Kilogram),
            ("remaining_fuel", Kilogram),
            ("final_mass", Kilogram),
            ("cost", Dollar),
        ],
        Kind::Operations => &[
            ("trip_cost", Dollar),
            ("casm", DollarPerNauticalMile),
            ("network_compatible", Ratio),
        ],
        Kind::TripCost => &[
            ("trip_cost", Dollar),
            ("cost_per_hour", DollarPerHour),
            ("network_compatible", Ratio),
        ],
        Kind::Phase => &[
            ("duration", Second),
            ("distance", Meter),
            ("fuel_burn", Kilogram),
            ("cost", Dollar),
        ],
    };
    ports
        .iter()
        .map(|(id, unit)| ((*id).into(), *unit))
        .collect()
}
