use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{ModelMetadata, PayloadRangePoint, PayloadRangeResult};
use crate::domain::schema::{EngineProfile, ResolvedScenario, SegmentKind};
use crate::models::aerodynamics::{FlightCondition, evaluate as evaluate_aerodynamics};
use crate::models::atmosphere::Isa1976;

#[derive(Debug, Clone)]
pub(crate) struct PayloadRangeAnalyzer {
    scenario: ResolvedScenario,
}

impl PayloadRangeAnalyzer {
    pub(crate) fn new(scenario: ResolvedScenario) -> Self {
        Self { scenario }
    }

    pub(crate) fn analyze(&self) -> AexResult<PayloadRangeResult> {
        let mass = &self.scenario.aircraft.mass;
        let maximum_payload = mass.maximum_payload_mass_kg;
        let maximum_fuel = mass.maximum_fuel_mass_kg;
        let fuel_at_maximum_payload =
            (mass.maximum_takeoff_mass_kg - mass.operating_empty_mass_kg - maximum_payload)
                .clamp(0.0, maximum_fuel);
        let payload_at_maximum_fuel =
            (mass.maximum_takeoff_mass_kg - mass.operating_empty_mass_kg - maximum_fuel)
                .clamp(0.0, maximum_payload);
        let reduced_payload = payload_at_maximum_fuel * 0.5;
        let definitions = [
            ("maximum_payload", maximum_payload, fuel_at_maximum_payload),
            (
                "maximum_fuel_payload",
                payload_at_maximum_fuel,
                maximum_fuel,
            ),
            ("maximum_range", reduced_payload, maximum_fuel),
            ("zero_payload_ferry", 0.0, maximum_fuel),
        ];
        let points = definitions
            .into_iter()
            .map(|(id, payload, fuel)| self.range_point(id, payload, fuel))
            .collect::<AexResult<Vec<_>>>()?;
        Ok(PayloadRangeResult {
            scenario_id: self.scenario.id.clone(),
            points,
            simplified_cruise_assumption: true,
            warnings: vec![Diagnostic::warning(
                "SIMPLIFIED_PAYLOAD_RANGE",
                "Payload-range values use a representative cruise condition, not a full mission.",
                "analysis.payload_range",
            )],
            model: ModelMetadata {
                model_id: "performance.payload_range_simple".to_owned(),
                model_version: "1.0.0".to_owned(),
                fidelity_level: 0,
                validity_status: "approximate".to_owned(),
            },
        })
    }

    fn range_point(&self, id: &str, payload_kg: f64, fuel_kg: f64) -> AexResult<PayloadRangePoint> {
        let empty = self.scenario.aircraft.mass.operating_empty_mass_kg;
        let start_mass = empty + payload_kg + fuel_kg;
        let reserve_fraction = 0.08;
        let usable_fuel = fuel_kg * (1.0 - reserve_fraction);
        let representative_mass = start_mass - 0.5 * usable_fuel;
        let (altitude, speed) = cruise_condition(&self.scenario)?;
        let fuel_flow =
            representative_cruise_fuel_flow(&self.scenario, altitude, speed, representative_mass)?;
        if fuel_flow <= 0.0 {
            return Err(AexError::analysis(
                "INVALID_FUEL_FLOW",
                "representative cruise fuel flow is not positive",
            ));
        }
        let range_m = usable_fuel / fuel_flow * speed;
        Ok(PayloadRangePoint {
            id: id.to_owned(),
            range: QuantityOutput::range(range_m),
            payload_kg,
            fuel_kg,
        })
    }
}

fn cruise_condition(scenario: &ResolvedScenario) -> AexResult<(f64, f64)> {
    let segment = scenario
        .mission
        .segments
        .iter()
        .find(|item| item.kind == SegmentKind::Cruise)
        .ok_or_else(|| AexError::analysis("NO_CRUISE_SEGMENT", "mission has no cruise segment"))?;
    let altitude = segment.altitude_m.unwrap_or(0.0);
    if let Some(speed) = segment.true_airspeed_m_s {
        return Ok((altitude, speed));
    }
    let atmosphere = Isa1976::new(0.0).evaluate(altitude)?;
    let speed = segment
        .mach
        .map(|mach| mach * atmosphere.speed_of_sound_m_s)
        .or(segment.indicated_airspeed_m_s)
        .ok_or_else(|| {
            AexError::analysis(
                "MISSING_CRUISE_SPEED",
                "cruise segment needs true airspeed, indicated airspeed, or Mach",
            )
        })?;
    Ok((altitude, speed))
}

fn representative_cruise_fuel_flow(
    scenario: &ResolvedScenario,
    altitude_m: f64,
    speed_m_s: f64,
    mass_kg: f64,
) -> AexResult<f64> {
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    let aero = evaluate_aerodynamics(
        &scenario.aircraft,
        "clean",
        FlightCondition {
            density_kg_m3: atmosphere.density_kg_m3,
            speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
            true_airspeed_m_s: speed_m_s,
            mass_kg,
        },
    )?;
    match &scenario.engine {
        EngineProfile::Piston(profile) => {
            let efficiency = scenario
                .propeller
                .as_ref()
                .map_or(0.75, |item| item.cruise_efficiency);
            let shaft_power_kw = aero.power_required_w / efficiency / 1000.0;
            Ok(profile.bsfc_cruise_kg_kwh * shaft_power_kw / 3600.0)
        }
        EngineProfile::Turbofan(profile) => Ok(profile.tsfc_cruise_kg_n_hr * aero.drag_n / 3600.0),
    }
}
