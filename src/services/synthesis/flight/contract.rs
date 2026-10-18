use super::PhaseKind;
use ace_model::{DomainBound, DomainLimit, DomainVariable, ScalarPort, Unit, ValidityDomain};

pub(super) fn domain(kind: PhaseKind, propeller: bool, polar: bool) -> ValidityDomain {
    use Unit::*;
    let mut bounds = vec![nonnegative(input("fuel_price"), DollarPerKilogram)];
    if matches!(kind, PhaseKind::GroundEstimate) {
        bounds.push(nonnegative(input("fuel_flow"), KilogramPerSecond));
        bounds.extend([
            DomainBound::positive(condition("duration"), Second),
            nonnegative(condition("mean_speed"), MeterPerSecond),
            nonnegative(condition("target_altitude"), Meter),
            nonnegative(condition("target_speed"), MeterPerSecond),
        ]);
    } else {
        for (name, unit) in [
            ("area", SquareMeter),
            ("span", Meter),
            ("cd0", Ratio),
            ("cl_max", Ratio),
            ("maximum_step", Second),
        ] {
            bounds.push(DomainBound::positive(input(name), unit));
        }
        propulsion(&mut bounds, propeller);
        if polar {
            for name in crate::backends::openvsp::polar::PORTS {
                bounds.push(DomainBound::positive(input(name), Ratio));
            }
        } else {
            let mut oswald = DomainBound::positive(input("oswald"), Ratio);
            oswald.upper = Some(DomainLimit {
                value: 1.0,
                inclusive: true,
            });
            bounds.push(oswald);
        }
        bounds.extend([
            DomainBound::closed(condition("altitude"), Meter, 0.0, 20_000.0),
            DomainBound::positive(condition("speed"), MeterPerSecond),
        ]);
        stage(&mut bounds, kind);
    }
    ValidityDomain { bounds, assumptions: vec![
        "Conceptual model; interval acceptance does not establish calibration or validation".into(),
        "Coupled lift, thrust, Mach, path-angle and integration-step limits are checked by the physical solver".into(),
    ] }
}

fn propulsion(bounds: &mut Vec<DomainBound>, propeller: bool) {
    use Unit::*;
    if propeller {
        for (name, unit) in [("shaft_power", Watt), ("bsfc", KilogramPerJoule)] {
            bounds.push(DomainBound::positive(input(name), unit));
        }
        let mut efficiency = DomainBound::positive(input("propeller_efficiency"), Ratio);
        efficiency.upper = Some(DomainLimit {
            value: 1.0,
            inclusive: true,
        });
        bounds.push(efficiency);
        bounds.push(DomainBound::closed(
            input("power_lapse_exponent"),
            Ratio,
            0.0,
            3.0,
        ));
    } else {
        bounds.extend([
            DomainBound::positive(input("available_thrust"), Newton),
            DomainBound::positive(input("tsfc"), KilogramPerNewtonSecond),
        ]);
    }
}

fn stage(bounds: &mut Vec<DomainBound>, kind: PhaseKind) {
    use Unit::*;
    match kind {
        PhaseKind::EnergyPath => bounds.extend([
            DomainBound::closed(condition("target_altitude"), Meter, 0.0, 20_000.0),
            DomainBound::positive(condition("target_speed"), MeterPerSecond),
            DomainBound::positive(condition("path_distance"), Meter),
        ]),
        PhaseKind::RouteCruise => bounds.extend([
            DomainBound::positive(condition("route_distance"), Meter),
            DomainBound::positive(condition("arrival_distance"), Meter),
        ]),
        PhaseKind::Level => bounds.push(nonnegative(condition("duration"), Second)),
        PhaseKind::GroundEstimate => {}
    }
}

fn input(name: &str) -> DomainVariable {
    DomainVariable::Input(name.into())
}
fn condition(name: &str) -> DomainVariable {
    DomainVariable::Condition(name.into())
}

fn nonnegative(variable: DomainVariable, unit: Unit) -> DomainBound {
    let mut bound = DomainBound::positive(variable, unit);
    bound.lower = Some(DomainLimit {
        value: 0.0,
        inclusive: true,
    });
    bound
}

pub(crate) fn polar_port(name: &str) -> ScalarPort {
    ScalarPort::semantic(
        Unit::Ratio,
        format!("aircraft.induced-polar.{name}/1"),
        "horizontal-projected-wing-area;CL=lift/(q*S);CDi=k*CL^2",
    )
}

#[cfg(test)]
mod tests;
