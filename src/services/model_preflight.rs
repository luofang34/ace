use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::ResolvedScenario;
use crate::domain::validity::{
    ModelDomainViolation, ModelValidityDomain, ValidityBound, ValidityVariable,
};
use crate::models::validity::{
    ModelDomainRole, aerodynamic_configuration_domain, scenario_domain_registrations,
};

mod mission;
mod speed;

use mission::mission_declarations;
use speed::{EffectiveSpeed, effective_speed};

#[derive(Debug)]
struct Declaration {
    scope: ModelDomainRole,
    variable: ValidityVariable,
    path: String,
    value: f64,
}

pub(crate) fn preflight_model_domains(scenario: &ResolvedScenario) -> AexResult<()> {
    let declarations = declarations(scenario)?;
    preflight_declarations(scenario, &declarations, None)
}

pub(crate) fn preflight_operating_point(
    scenario: &ResolvedScenario,
    configuration: &str,
    altitude_m: f64,
    speed_m_s: Option<f64>,
    mach: Option<f64>,
    mass_kg: f64,
) -> AexResult<()> {
    let mut declarations = Vec::new();
    let speed = effective_speed(altitude_m, speed_m_s, mach, None)?;
    push_scopes(
        &mut declarations,
        &[ModelDomainRole::Atmosphere, ModelDomainRole::Propulsion],
        ValidityVariable::Altitude,
        "condition.altitude",
        altitude_m,
    );
    push_effective_speed(&mut declarations, "condition", speed);
    push_scopes(
        &mut declarations,
        &[ModelDomainRole::Aerodynamics],
        ValidityVariable::Mass,
        "condition.mass",
        mass_kg,
    );
    let aerodynamic_domain = aerodynamic_configuration_domain(scenario, configuration)?;
    preflight_declarations(scenario, &declarations, Some(&aerodynamic_domain))
}

fn preflight_declarations(
    scenario: &ResolvedScenario,
    declarations: &[Declaration],
    aerodynamic_override: Option<&ModelValidityDomain>,
) -> AexResult<()> {
    reject_non_finite_declaration(declarations)?;
    let mut violations = Vec::new();
    for registration in scenario_domain_registrations(scenario)? {
        if registration.role == ModelDomainRole::ConditionalAerodynamics {
            continue;
        }
        if aerodynamic_override.is_some() && registration.role == ModelDomainRole::Aerodynamics {
            continue;
        }
        collect_domain_violations(
            &registration.domain,
            registration.role,
            declarations,
            &mut violations,
        );
    }
    if let Some(domain) = aerodynamic_override {
        collect_domain_violations(
            domain,
            ModelDomainRole::Aerodynamics,
            declarations,
            &mut violations,
        );
    }
    violations.sort_by(|left, right| {
        (&left.path, &left.model_id, left.variable).cmp(&(
            &right.path,
            &right.model_id,
            right.variable,
        ))
    });
    if violations.is_empty() {
        Ok(())
    } else {
        Err(AexError::ModelDomainUnsupported {
            message: violation_summary(&violations),
            violations,
        })
    }
}

fn declarations(scenario: &ResolvedScenario) -> AexResult<Vec<Declaration>> {
    let mut values = Vec::new();
    aircraft_declarations(scenario, &mut values);
    mission_declarations(scenario, &mut values)?;
    Ok(values)
}

fn aircraft_declarations(scenario: &ResolvedScenario, values: &mut Vec<Declaration>) {
    let aircraft = &scenario.aircraft;
    aircraft_mass_declarations(scenario, values);
    push(
        values,
        ModelDomainRole::Structure,
        ValidityVariable::AspectRatio,
        "aircraft.geometry.wing.aspect_ratio",
        aircraft.wing.aspect_ratio,
    );
    push_optional_limit(
        values,
        aircraft.limits.maximum_operating_altitude_m,
        ValidityVariable::Altitude,
    );
    push_optional_limit(
        values,
        aircraft.limits.maximum_operating_mach,
        ValidityVariable::Mach,
    );
    push_optional_limit(
        values,
        aircraft.limits.maximum_operating_speed_m_s,
        ValidityVariable::TrueAirspeed,
    );
    if let Some(value) = aircraft.limits.maximum_load_factor {
        push(
            values,
            ModelDomainRole::Structure,
            ValidityVariable::LoadFactor,
            "aircraft.limits.maximum_load_factor",
            value,
        );
    }
}

fn aircraft_mass_declarations(scenario: &ResolvedScenario, values: &mut Vec<Declaration>) {
    let mass = &scenario.aircraft.mass;
    push_scopes(
        values,
        &[
            ModelDomainRole::Aerodynamics,
            ModelDomainRole::FieldPerformance,
            ModelDomainRole::Structure,
        ],
        ValidityVariable::Mass,
        "aircraft.mass.maximum_takeoff_mass",
        mass.maximum_takeoff_mass_kg,
    );
    for (path, value) in [
        (
            "aircraft.mass.operating_empty_mass",
            mass.operating_empty_mass_kg,
        ),
        (
            "aircraft.mass.maximum_payload_mass",
            mass.maximum_payload_mass_kg,
        ),
        ("aircraft.mass.maximum_fuel_mass", mass.maximum_fuel_mass_kg),
    ] {
        push_scopes(
            values,
            &[ModelDomainRole::Aerodynamics, ModelDomainRole::Structure],
            ValidityVariable::Mass,
            path,
            value,
        );
    }
}

fn push_optional_limit(
    values: &mut Vec<Declaration>,
    value: Option<f64>,
    variable: ValidityVariable,
) {
    let Some(value) = value else {
        return;
    };
    let (path, scopes): (&str, &[ModelDomainRole]) = match variable {
        ValidityVariable::Altitude => (
            "aircraft.limits.maximum_operating_altitude",
            &[ModelDomainRole::Atmosphere, ModelDomainRole::Propulsion],
        ),
        ValidityVariable::Mach => (
            "aircraft.limits.maximum_operating_mach",
            &[ModelDomainRole::Aerodynamics, ModelDomainRole::Propulsion],
        ),
        ValidityVariable::TrueAirspeed => (
            "aircraft.limits.maximum_operating_speed",
            &[ModelDomainRole::Aerodynamics],
        ),
        _ => return,
    };
    push_scopes(values, scopes, variable, path, value);
}

fn push_effective_speed(values: &mut Vec<Declaration>, root: &str, speed: Option<EffectiveSpeed>) {
    let Some(speed) = speed else {
        return;
    };
    push_segment_value(
        values,
        root,
        speed.source_field,
        speed.true_airspeed_m_s,
        ValidityVariable::TrueAirspeed,
        &[ModelDomainRole::Aerodynamics],
    );
    push_segment_value(
        values,
        root,
        speed.source_field,
        speed.mach,
        ValidityVariable::Mach,
        &[ModelDomainRole::Aerodynamics, ModelDomainRole::Propulsion],
    );
}

fn push_segment_value(
    values: &mut Vec<Declaration>,
    root: &str,
    field: &str,
    value: Option<f64>,
    variable: ValidityVariable,
    scopes: &[ModelDomainRole],
) {
    if let Some(value) = value {
        push_scopes(values, scopes, variable, &format!("{root}.{field}"), value);
    }
}

fn push_scopes(
    values: &mut Vec<Declaration>,
    scopes: &[ModelDomainRole],
    variable: ValidityVariable,
    path: &str,
    value: f64,
) {
    for scope in scopes {
        push(values, *scope, variable, path, value);
    }
}

fn push(
    values: &mut Vec<Declaration>,
    scope: ModelDomainRole,
    variable: ValidityVariable,
    path: &str,
    value: f64,
) {
    values.push(Declaration {
        scope,
        variable,
        path: path.to_owned(),
        value,
    });
}

fn collect_domain_violations(
    domain: &ModelValidityDomain,
    scope: ModelDomainRole,
    declarations: &[Declaration],
    violations: &mut Vec<ModelDomainViolation>,
) {
    for bound in &domain.bounds {
        for declared in declarations
            .iter()
            .filter(|value| value.scope == scope && value.variable == bound.variable)
        {
            if outside(declared.value, bound) {
                violations.push(violation(domain, bound, declared));
            }
        }
    }
}

fn outside(value: f64, bound: &ValidityBound) -> bool {
    let below = bound
        .minimum
        .is_some_and(|minimum| value < minimum || !bound.minimum_inclusive && value == minimum);
    let above = bound
        .maximum
        .is_some_and(|maximum| value > maximum || !bound.maximum_inclusive && value == maximum);
    below || above
}

fn reject_non_finite_declaration(declarations: &[Declaration]) -> AexResult<()> {
    let invalid = declarations
        .iter()
        .filter(|declaration| !declaration.value.is_finite())
        .min_by(|left, right| (&left.path, left.variable).cmp(&(&right.path, right.variable)));
    if let Some(declaration) = invalid {
        Err(AexError::validation(
            "NON_FINITE_VALUE",
            &declaration.path,
            "declared condition must be finite",
        ))
    } else {
        Ok(())
    }
}

fn violation(
    domain: &ModelValidityDomain,
    bound: &ValidityBound,
    declared: &Declaration,
) -> ModelDomainViolation {
    ModelDomainViolation {
        path: declared.path.clone(),
        model_id: domain.model_id.clone(),
        variable: bound.variable,
        declared_value: declared.value,
        declared_unit: declared.variable.unit().to_owned(),
        minimum: bound.minimum,
        maximum: bound.maximum,
        minimum_inclusive: bound.minimum_inclusive,
        maximum_inclusive: bound.maximum_inclusive,
        bound_unit: bound.unit.clone(),
        basis: bound.basis,
    }
}

fn violation_summary(violations: &[ModelDomainViolation]) -> String {
    let entries = violations
        .iter()
        .map(|item| format!("{} for {}", item.path, item.model_id))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "{} declared condition(s) exceed registered model domains: {entries}",
        violations.len()
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod point_tests;
