use std::collections::BTreeMap;

use crate::domain::diagnostic::Diagnostic;
use crate::domain::quantity::{GRAVITY_M_S2, QuantityOutput};
use crate::domain::result::{ResultProvenance, StructuralScreen};
use crate::domain::schema::ResolvedScenario;
use crate::models::concept_geometry::ConceptGeometry;

const ALLOWABLE_CAP_STRESS_PA: f64 = 240.0e6;
const MAXIMUM_CAP_PACKAGING_RATIO: f64 = 0.65;

pub(crate) fn evaluate(scenario: &ResolvedScenario) -> StructuralScreen {
    let aircraft = &scenario.aircraft;
    let wing = &aircraft.wing;
    let concept = ConceptGeometry::from_scenario(scenario);
    let limit_factor = aircraft.limits.maximum_load_factor.unwrap_or(2.5);
    let ultimate_factor = limit_factor * 1.5;
    let bending_moment =
        ultimate_factor * aircraft.mass.maximum_takeoff_mass_kg * GRAVITY_M_S2 * wing.span_m / 8.0;
    let root_chord = 2.0 * wing.area_m2 / (wing.span_m * (1.0 + concept.taper_ratio));
    let spar_depth = 0.09 * root_chord;
    let required_cap_area = 2.0 * bending_moment / (ALLOWABLE_CAP_STRESS_PA * spar_depth);
    let available_cap_area = 0.004 * root_chord.powi(2);
    let packaging_ratio = required_cap_area / available_cap_area;
    let tail_arm = concept.tail_arm_m();
    let mean_chord = wing.area_m2 / wing.span_m;
    let horizontal_volume =
        concept.horizontal_tail_area_m2 * tail_arm / (wing.area_m2 * mean_chord);
    let vertical_volume = concept.vertical_tail_area_m2 * tail_arm / (wing.area_m2 * wing.span_m);
    let transport = aircraft.category.contains("transport");
    let mut failed = Vec::new();
    if packaging_ratio > MAXIMUM_CAP_PACKAGING_RATIO {
        failed.push("structures.wing_spar_packaging".to_owned());
    }
    let horizontal_bounds = if transport { (0.5, 1.2) } else { (0.35, 0.9) };
    if !(horizontal_bounds.0..=horizontal_bounds.1).contains(&horizontal_volume) {
        failed.push("stability.horizontal_tail_volume".to_owned());
    }
    if !(0.02..=0.08).contains(&vertical_volume) {
        failed.push("stability.vertical_tail_volume".to_owned());
    }
    let aspect_ratio_lower = if transport { 7.0 } else { 5.0 };
    if !(aspect_ratio_lower..=12.0).contains(&wing.aspect_ratio) {
        failed.push("geometry.aspect_ratio".to_owned());
    }
    StructuralScreen {
        passed: failed.is_empty(),
        ultimate_load_factor: ultimate_factor,
        wing_root_bending_moment: QuantityOutput::si(bending_moment, "N*m"),
        required_total_spar_cap_area: QuantityOutput::si(required_cap_area, "m^2"),
        spar_cap_packaging_ratio: packaging_ratio,
        horizontal_tail_volume: horizontal_volume,
        vertical_tail_volume: vertical_volume,
        aspect_ratio: wing.aspect_ratio,
        failed_constraints: failed,
        provenance: provenance(),
    }
}

fn provenance() -> ResultProvenance {
    ResultProvenance {
        method: "cantilever wing bending and empirical tail-volume screening".to_owned(),
        backend: "native".to_owned(),
        assumptions: vec![
            "elliptic-to-uniform conservative root bending moment W n b / 8".to_owned(),
            "1.5 ultimate factor on the declared positive limit load".to_owned(),
            "240 MPa spar-cap allowable and 9% root-chord spar depth".to_owned(),
            "spar-cap packaging allowance is 0.4% of root-chord squared".to_owned(),
        ],
        validity_range: vec![
            "conventional cantilever fixed-wing layouts".to_owned(),
            "conceptual structural packaging only".to_owned(),
        ],
        units: BTreeMap::from([
            ("bending_moment".to_owned(), "N*m".to_owned()),
            ("area".to_owned(), "m^2".to_owned()),
            ("volume_coefficients".to_owned(), "1".to_owned()),
        ]),
        warnings: vec![Diagnostic::limitation(
            "This screen does not model detailed loads, joints, fatigue, buckling, flutter, or aeroelasticity.",
        )],
    }
}
