use crate::domain::diagnostic::AexResult;
use crate::domain::propulsion::TABLE_PROPULSION_MODEL_ID;
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::validity::{
    ModelValidityDomain, ValidityBasis, ValidityBound, ValidityVariable,
};
use crate::models::atmosphere::Isa1976;
use crate::models::blended_wing::is_blended_wing_body;

pub(crate) use crate::domain::validity::ValidityDomainProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegisteredModel {
    Atmosphere,
    ParabolicPolar,
    WaveDrag,
    PistonPropulsion,
    TurbofanPropulsion,
    TablePropulsion,
    FieldPerformance,
    ConventionalStructure,
    BlendedWingStructure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelDomainRole {
    Atmosphere,
    Aerodynamics,
    Propulsion,
    FieldPerformance,
    Structure,
    ConditionalAerodynamics,
}

#[derive(Debug, Clone)]
pub(crate) struct ScenarioDomainRegistration {
    pub(crate) role: ModelDomainRole,
    pub(crate) domain: ModelValidityDomain,
}

impl RegisteredModel {
    pub(crate) const ALL: [Self; 9] = [
        Self::Atmosphere,
        Self::ParabolicPolar,
        Self::WaveDrag,
        Self::PistonPropulsion,
        Self::TurbofanPropulsion,
        Self::TablePropulsion,
        Self::FieldPerformance,
        Self::ConventionalStructure,
        Self::BlendedWingStructure,
    ];

    pub(crate) fn model_id(self) -> &'static str {
        match self {
            Self::Atmosphere => "atmosphere.isa1976",
            Self::ParabolicPolar => "aero.parabolic_polar",
            Self::WaveDrag => "aero.wave_drag_power_law",
            Self::PistonPropulsion => "propulsion.piston_prop_simple",
            Self::TurbofanPropulsion => "propulsion.turbofan_simple_deck",
            Self::TablePropulsion => TABLE_PROPULSION_MODEL_ID,
            Self::FieldPerformance => "performance.field_length_simple",
            Self::ConventionalStructure => "structures.conventional_conceptual_screen",
            Self::BlendedWingStructure => "structures.blended_wing_conceptual_screen",
        }
    }
}

impl ValidityDomainProvider for RegisteredModel {
    fn validity_domain(&self) -> ModelValidityDomain {
        match self {
            Self::Atmosphere => atmosphere_domain(),
            Self::ParabolicPolar => polar_domain(),
            Self::WaveDrag => wave_drag_domain(0.0),
            Self::PistonPropulsion => generic_propulsion_domain(self.model_id(), None),
            Self::TurbofanPropulsion => generic_turbofan_domain(self.model_id(), None, None),
            Self::TablePropulsion => generic_turbofan_domain(self.model_id(), None, None),
            Self::FieldPerformance => field_performance_domain(),
            Self::ConventionalStructure => structural_domain(false),
            Self::BlendedWingStructure => structural_domain(true),
        }
    }
}

impl ValidityDomainProvider for Isa1976 {
    fn validity_domain(&self) -> ModelValidityDomain {
        atmosphere_domain()
    }
}

impl ValidityDomainProvider for EngineProfile {
    fn validity_domain(&self) -> ModelValidityDomain {
        match self {
            Self::Piston(profile) => {
                generic_propulsion_domain(&profile.model, profile.maximum_altitude_m)
            }
            Self::Turbofan(profile) => {
                if let Some(deck) = &profile.table_deck {
                    table_propulsion_domain(&profile.model, deck)
                } else {
                    profile.simple_deck.as_ref().map_or_else(
                        || generic_turbofan_domain(&profile.model, None, None),
                        |deck| {
                            generic_turbofan_domain(
                                &profile.model,
                                Some(deck.maximum_altitude_m),
                                Some(deck.maximum_mach),
                            )
                        },
                    )
                }
            }
        }
    }
}

pub(crate) fn scenario_domains(scenario: &ResolvedScenario) -> AexResult<Vec<ModelValidityDomain>> {
    Ok(scenario_domain_registrations(scenario)?
        .into_iter()
        .map(|registration| registration.domain)
        .collect())
}

pub(crate) fn registered_model_domains() -> AexResult<Vec<ModelValidityDomain>> {
    validate_registered_domains()?;
    Ok(RegisteredModel::ALL
        .into_iter()
        .map(|model| model.validity_domain())
        .collect())
}

pub(crate) fn scenario_domain_registrations(
    scenario: &ResolvedScenario,
) -> AexResult<Vec<ScenarioDomainRegistration>> {
    validate_registered_domains()?;
    let mut domains = vec![
        registration(ModelDomainRole::Atmosphere, atmosphere_domain()),
        registration(ModelDomainRole::Aerodynamics, polar_domain()),
    ];
    if let Some(critical) = minimum_wave_drag_mach(scenario) {
        domains.push(registration(
            ModelDomainRole::ConditionalAerodynamics,
            wave_drag_domain(critical),
        ));
    }
    domains.extend([
        registration(
            ModelDomainRole::Propulsion,
            scenario.engine.validity_domain(),
        ),
        registration(
            ModelDomainRole::FieldPerformance,
            field_performance_domain(),
        ),
        registration(
            ModelDomainRole::Structure,
            structural_domain(is_blended_wing_body(scenario)),
        ),
    ]);
    for registration in &domains {
        registration.domain.validate()?;
    }
    Ok(domains)
}

fn validate_registered_domains() -> AexResult<()> {
    RegisteredModel::ALL
        .iter()
        .try_for_each(|model| model.validity_domain().validate())
}

fn registration(role: ModelDomainRole, domain: ModelValidityDomain) -> ScenarioDomainRegistration {
    ScenarioDomainRegistration { role, domain }
}

pub(crate) fn structural_domain(blended: bool) -> ModelValidityDomain {
    ModelValidityDomain {
        model_id: if blended {
            RegisteredModel::BlendedWingStructure.model_id()
        } else {
            RegisteredModel::ConventionalStructure.model_id()
        }
        .to_owned(),
        bounds: vec![
            lower_exclusive_bound(ValidityVariable::Mass, 0.0, ValidityBasis::ModelForm),
            inclusive_bound(
                ValidityVariable::AspectRatio,
                Some(5.0),
                Some(12.0),
                ValidityBasis::ScreeningAssumption,
            ),
            lower_exclusive_bound(ValidityVariable::LoadFactor, 0.0, ValidityBasis::ModelForm),
        ],
    }
}

fn atmosphere_domain() -> ModelValidityDomain {
    ModelValidityDomain {
        model_id: RegisteredModel::Atmosphere.model_id().to_owned(),
        bounds: vec![inclusive_bound(
            ValidityVariable::Altitude,
            Some(-2_000.0),
            Some(20_000.0),
            ValidityBasis::PublishedSpecification,
        )],
    }
}

fn polar_domain() -> ModelValidityDomain {
    ModelValidityDomain {
        model_id: RegisteredModel::ParabolicPolar.model_id().to_owned(),
        bounds: vec![
            inclusive_bound(
                ValidityVariable::Mach,
                Some(0.0),
                Some(0.9),
                ValidityBasis::ModelForm,
            ),
            lower_exclusive_bound(
                ValidityVariable::TrueAirspeed,
                0.0,
                ValidityBasis::ModelForm,
            ),
            lower_exclusive_bound(ValidityVariable::Mass, 0.0, ValidityBasis::ModelForm),
        ],
    }
}

fn wave_drag_domain(minimum_mach: f64) -> ModelValidityDomain {
    ModelValidityDomain {
        model_id: RegisteredModel::WaveDrag.model_id().to_owned(),
        bounds: vec![inclusive_bound(
            ValidityVariable::Mach,
            Some(minimum_mach),
            None,
            ValidityBasis::ModelForm,
        )],
    }
}

fn generic_propulsion_domain(
    model_id: &str,
    maximum_altitude_m: Option<f64>,
) -> ModelValidityDomain {
    ModelValidityDomain {
        model_id: model_id.to_owned(),
        bounds: vec![
            inclusive_bound(
                ValidityVariable::Altitude,
                Some(0.0),
                maximum_altitude_m,
                ValidityBasis::ResolvedProfile,
            ),
            inclusive_bound(
                ValidityVariable::Throttle,
                Some(0.0),
                Some(1.0),
                ValidityBasis::ModelForm,
            ),
        ],
    }
}

fn generic_turbofan_domain(
    model_id: &str,
    maximum_altitude_m: Option<f64>,
    maximum_mach: Option<f64>,
) -> ModelValidityDomain {
    let mut domain = generic_propulsion_domain(model_id, maximum_altitude_m);
    domain.bounds.insert(
        1,
        inclusive_bound(
            ValidityVariable::Mach,
            Some(0.0),
            maximum_mach,
            ValidityBasis::ResolvedProfile,
        ),
    );
    domain
}

fn table_propulsion_domain(
    model_id: &str,
    deck: &crate::domain::propulsion::TablePropulsionDeck,
) -> ModelValidityDomain {
    ModelValidityDomain {
        model_id: model_id.to_owned(),
        bounds: vec![
            inclusive_bound(
                ValidityVariable::Altitude,
                deck.altitude_axis_m.first().copied(),
                deck.altitude_axis_m.last().copied(),
                ValidityBasis::TabulatedData,
            ),
            inclusive_bound(
                ValidityVariable::Mach,
                deck.mach_axis.first().copied(),
                deck.mach_axis.last().copied(),
                ValidityBasis::TabulatedData,
            ),
            inclusive_bound(
                ValidityVariable::Throttle,
                Some(0.0),
                Some(1.0),
                ValidityBasis::ModelForm,
            ),
        ],
    }
}

fn field_performance_domain() -> ModelValidityDomain {
    ModelValidityDomain {
        model_id: RegisteredModel::FieldPerformance.model_id().to_owned(),
        bounds: vec![
            inclusive_bound(
                ValidityVariable::Altitude,
                Some(0.0),
                Some(0.0),
                ValidityBasis::ScreeningAssumption,
            ),
            lower_exclusive_bound(ValidityVariable::Mass, 0.0, ValidityBasis::ModelForm),
        ],
    }
}

fn minimum_wave_drag_mach(scenario: &ResolvedScenario) -> Option<f64> {
    [
        &scenario.aircraft.aerodynamics.clean,
        &scenario.aircraft.aerodynamics.takeoff,
        &scenario.aircraft.aerodynamics.landing,
    ]
    .into_iter()
    .filter(|configuration| configuration.wave_drag.is_some())
    .filter_map(|configuration| configuration.mach_critical)
    .reduce(f64::min)
}

fn inclusive_bound(
    variable: ValidityVariable,
    minimum: Option<f64>,
    maximum: Option<f64>,
    basis: ValidityBasis,
) -> ValidityBound {
    ValidityBound {
        variable,
        minimum,
        maximum,
        minimum_inclusive: true,
        maximum_inclusive: true,
        unit: variable.unit().to_owned(),
        basis,
    }
}

fn lower_exclusive_bound(
    variable: ValidityVariable,
    minimum: f64,
    basis: ValidityBasis,
) -> ValidityBound {
    ValidityBound {
        minimum_inclusive: false,
        ..inclusive_bound(variable, Some(minimum), None, basis)
    }
}

#[cfg(test)]
mod tests;
