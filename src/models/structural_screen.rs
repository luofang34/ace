use std::collections::BTreeMap;

use crate::domain::diagnostic::{AexResult, Diagnostic};
use crate::domain::quantity::{GRAVITY_M_S2, QuantityOutput};
use crate::domain::result::{ResultProvenance, StructuralScreen};
use crate::domain::schema::ResolvedScenario;
use crate::models::blended_wing::{BlendedWingPlanform, is_blended_wing_body};
use crate::models::concept_geometry::ConceptGeometry;
use crate::models::validity::structural_domain;

const ALLOWABLE_CAP_STRESS_PA: f64 = 240.0e6;
const MAXIMUM_CAP_PACKAGING_RATIO: f64 = 0.65;

const MAXIMUM_FUEL_VOLUME_UTILIZATION: f64 = 0.80;
const JET_FUEL_DENSITY_KG_M3: f64 = 800.0;

pub(crate) fn evaluate(scenario: &ResolvedScenario) -> AexResult<StructuralScreen> {
    if is_blended_wing_body(scenario) {
        return blended_wing_screen(scenario);
    }
    Ok(conventional_screen(scenario))
}

fn conventional_screen(scenario: &ResolvedScenario) -> StructuralScreen {
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
        structural_configuration: "conventional_cantilever".to_owned(),
        ultimate_load_factor: ultimate_factor,
        wing_root_bending_moment: QuantityOutput::si(bending_moment, "N*m"),
        required_total_spar_cap_area: QuantityOutput::si(required_cap_area, "m^2"),
        spar_cap_packaging_ratio: packaging_ratio,
        horizontal_tail_volume: horizontal_volume,
        vertical_tail_volume: vertical_volume,
        aspect_ratio: wing.aspect_ratio,
        estimated_usable_internal_volume: None,
        required_fuel_volume: None,
        fuel_volume_utilization_ratio: None,
        failed_constraints: failed,
        provenance: conventional_provenance(),
    }
}

fn blended_wing_screen(scenario: &ResolvedScenario) -> AexResult<StructuralScreen> {
    let aircraft = &scenario.aircraft;
    let wing = &aircraft.wing;
    let planform = BlendedWingPlanform::from_wing(wing)?;
    let ultimate_factor = aircraft.limits.maximum_load_factor.unwrap_or(2.5) * 1.5;
    let outer_lift = aircraft.mass.maximum_takeoff_mass_kg
        * GRAVITY_M_S2
        * ultimate_factor
        * planform.outer_panel_area_m2()
        / wing.area_m2;
    let bending_moment = 0.5 * outer_lift * planform.outer_panel_lift_centroid_m();
    let spar_depth = 0.09 * planform.middle_chord_m;
    let required_cap_area = 2.0 * bending_moment / (ALLOWABLE_CAP_STRESS_PA * spar_depth);
    let available_cap_area = 0.004 * planform.middle_chord_m.powi(2);
    let packaging_ratio = required_cap_area / available_cap_area;
    let usable_volume = planform.estimated_usable_volume_m3();
    let required_fuel_volume = aircraft.mass.maximum_fuel_mass_kg / JET_FUEL_DENSITY_KG_M3;
    let volume_ratio = required_fuel_volume / usable_volume;
    let mut failed = Vec::new();
    if packaging_ratio > MAXIMUM_CAP_PACKAGING_RATIO {
        failed.push("structures.center_body_spar_packaging".to_owned());
    }
    if volume_ratio > MAXIMUM_FUEL_VOLUME_UTILIZATION {
        failed.push("structures.fuel_volume_packaging".to_owned());
    }
    if !(5.0..=12.0).contains(&wing.aspect_ratio) {
        failed.push("geometry.aspect_ratio".to_owned());
    }
    Ok(StructuralScreen {
        passed: failed.is_empty(),
        structural_configuration: "blended_wing_outer_panel".to_owned(),
        ultimate_load_factor: ultimate_factor,
        wing_root_bending_moment: QuantityOutput::si(bending_moment, "N*m"),
        required_total_spar_cap_area: QuantityOutput::si(required_cap_area, "m^2"),
        spar_cap_packaging_ratio: packaging_ratio,
        horizontal_tail_volume: 0.0,
        vertical_tail_volume: 0.0,
        aspect_ratio: wing.aspect_ratio,
        estimated_usable_internal_volume: Some(QuantityOutput::si(usable_volume, "m^3")),
        required_fuel_volume: Some(QuantityOutput::si(required_fuel_volume, "m^3")),
        fuel_volume_utilization_ratio: Some(volume_ratio),
        failed_constraints: failed,
        provenance: blended_wing_provenance(),
    })
}

fn conventional_provenance() -> ResultProvenance {
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
        validity_domains: vec![structural_domain(false)],
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

fn blended_wing_provenance() -> ResultProvenance {
    ResultProvenance {
        method: "outer-panel bending and internal fuel-volume packaging screen".to_owned(),
        backend: "native".to_owned(),
        assumptions: vec![
            "outer-panel lift scales with its planform-area fraction".to_owned(),
            "outer-panel lift acts at its trapezoidal area centroid".to_owned(),
            "240 MPa spar-cap allowable and 9% junction-chord spar depth".to_owned(),
            "45% of section-integrated wing volume is usable".to_owned(),
            "jet fuel density is 800 kg/m^3".to_owned(),
        ],
        validity_range: vec![
            "tailless blended-wing conceptual layouts".to_owned(),
            "conceptual load-path and volume packaging only".to_owned(),
        ],
        validity_domains: vec![structural_domain(true)],
        units: BTreeMap::from([
            ("bending_moment".to_owned(), "N*m".to_owned()),
            ("area".to_owned(), "m^2".to_owned()),
            ("volume".to_owned(), "m^3".to_owned()),
        ]),
        warnings: vec![Diagnostic::limitation(
            "The BWB screen omits pressure-cabin integration, torsion, cutouts, joints, buckling, fatigue, flutter, aeroelasticity, and detailed load cases.",
        )],
    }
}

#[cfg(test)]
mod tests;
