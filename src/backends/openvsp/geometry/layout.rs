use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::models::concept_geometry::ConceptGeometry;

const PISTON_LENGTH_SCALE_M_PER_KG_CUBE_ROOT: f64 = 0.30;
const PISTON_DIAMETER_SCALE_M_PER_KG_CUBE_ROOT: f64 = 0.17;
const TURBOFAN_LENGTH_SCALE_M_PER_KG_CUBE_ROOT: f64 = 0.34;
const TURBOFAN_DIAMETER_SCALE_M_PER_KG_CUBE_ROOT: f64 = 0.17;

#[derive(Debug, Clone, Copy)]
pub(super) struct EngineEnvelope {
    pub(super) length_m: f64,
    pub(super) diameter_m: f64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PropellerPlacement {
    pub(super) diameter_m: f64,
    pub(super) blade_count: u32,
    pub(super) x_m: f64,
    pub(super) z_m: f64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ConventionalPropulsionLayout {
    pub(super) engine: EngineEnvelope,
    pub(super) engine_count: u32,
    pub(super) engine_x_m: f64,
    pub(super) engine_y_m: f64,
    pub(super) engine_z_m: f64,
    pub(super) propeller: Option<PropellerPlacement>,
}

pub(super) fn engine_envelope(scenario: &ResolvedScenario) -> EngineEnvelope {
    match &scenario.engine {
        EngineProfile::Turbofan(profile) => {
            let mass_scale = profile.dry_mass_kg.cbrt();
            EngineEnvelope {
                length_m: profile
                    .overall_length_m
                    .unwrap_or(TURBOFAN_LENGTH_SCALE_M_PER_KG_CUBE_ROOT * mass_scale),
                diameter_m: profile
                    .maximum_diameter_m
                    .unwrap_or(TURBOFAN_DIAMETER_SCALE_M_PER_KG_CUBE_ROOT * mass_scale),
            }
        }
        EngineProfile::Piston(profile) => {
            let mass_scale = profile.dry_mass_kg.cbrt();
            EngineEnvelope {
                length_m: PISTON_LENGTH_SCALE_M_PER_KG_CUBE_ROOT * mass_scale,
                diameter_m: PISTON_DIAMETER_SCALE_M_PER_KG_CUBE_ROOT * mass_scale,
            }
        }
    }
}

pub(super) fn conventional_propulsion_layout(
    scenario: &ResolvedScenario,
    concept: ConceptGeometry,
) -> ConventionalPropulsionLayout {
    let wing = &scenario.aircraft.wing;
    let engine = engine_envelope(scenario);
    let propeller_profile = scenario.propeller.as_ref();
    let root_chord = 2.0 * wing.area_m2 / (wing.span_m * (1.0 + concept.taper_ratio));
    let engine_x_m = propeller_profile.map_or(concept.wing_x_m + 0.20 * root_chord, |_| 0.0);
    let engine_z_m = propeller_profile.map_or(concept.wing_z_m - 0.65 * engine.diameter_m, |_| 0.0);
    let propeller = propeller_profile.map(|profile| PropellerPlacement {
        diameter_m: profile.diameter_m,
        blade_count: profile.blade_count,
        x_m: engine_x_m - 0.03,
        z_m: engine_z_m,
    });
    ConventionalPropulsionLayout {
        engine,
        engine_count: scenario.aircraft.propulsion.engine_count,
        engine_x_m,
        engine_y_m: wing.span_m * 0.22,
        engine_z_m,
        propeller,
    }
}
