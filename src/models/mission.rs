use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{MissionResult, MissionSegmentResult, ModelMetadata};
use crate::domain::schema::{EngineProfile, MissionSegment, ResolvedScenario, SegmentKind};
use crate::models::atmosphere::Isa1976;
use crate::models::performance::PointAnalyzer;
use crate::models::propulsion::OperatingMode;

mod fuel;

#[derive(Debug, Clone)]
pub(crate) struct MissionSimulator {
    scenario: ResolvedScenario,
    atmosphere: Isa1976,
}

#[derive(Debug, Clone, Copy)]
struct MissionState {
    mass_kg: f64,
    altitude_m: f64,
    fuel_remaining_kg: f64,
    payload_remaining_kg: f64,
}

#[derive(Debug, Clone)]
struct SegmentComputation {
    fuel_burn_kg: f64,
    payload_removed_kg: f64,
    distance_m: f64,
    duration_s: f64,
    end_altitude_m: f64,
    warnings: Vec<Diagnostic>,
}

#[derive(Debug)]
struct InitialFuelLoad {
    loaded_kg: f64,
    capacity_exceeded: bool,
    warning: Option<Diagnostic>,
}

impl MissionSimulator {
    pub(crate) fn new(scenario: ResolvedScenario) -> Self {
        Self {
            scenario,
            atmosphere: Isa1976::new(0.0),
        }
    }

    pub(crate) fn simulate(&self) -> AexResult<MissionResult> {
        let aircraft = &self.scenario.aircraft;
        let fuel_capacity = aircraft.mass.maximum_fuel_mass_kg;
        let structural_fuel = (aircraft.mass.maximum_takeoff_mass_kg
            - aircraft.mass.operating_empty_mass_kg
            - self.scenario.mission.payload_mass_kg)
            .max(0.0);
        let InitialFuelLoad {
            loaded_kg: initial_fuel,
            capacity_exceeded,
            warning: fuel_load_warning,
        } = initial_fuel_load(fuel_capacity.min(structural_fuel), fuel_capacity);
        let mut state = MissionState {
            mass_kg: aircraft.mass.operating_empty_mass_kg
                + self.scenario.mission.payload_mass_kg
                + initial_fuel,
            altitude_m: 0.0,
            fuel_remaining_kg: initial_fuel,
            payload_remaining_kg: self.scenario.mission.payload_mass_kg,
        };
        let initial_takeoff_mass = state.mass_kg;
        let mut segment_results = Vec::new();
        let mut total_distance = 0.0;
        let mut total_duration = 0.0;
        let mut total_fuel = 0.0;
        let mut failed_segment = None;
        let mut fuel_exhausted = false;
        let mut warnings = self.scenario.warnings.clone();
        warnings.extend(fuel_load_warning);
        for segment in &self.scenario.mission.segments {
            let computation = self.compute_segment(segment, state)?;
            let segment_warnings = scoped_segment_warnings(segment, &computation.warnings);
            warnings.extend(segment_warnings.iter().cloned());
            if computation.fuel_burn_kg > state.fuel_remaining_kg + 1.0e-8 {
                failed_segment = Some(segment.id.clone());
                fuel_exhausted = true;
                warnings.push(Diagnostic::warning(
                    "FUEL_EXHAUSTED",
                    format!("Fuel was exhausted during segment {}.", segment.id),
                    format!("mission.segments.{}", segment.id),
                ));
                break;
            }
            segment_results.push(segment_result(
                segment,
                state,
                &computation,
                segment_warnings,
            ));
            state.mass_kg -= computation.fuel_burn_kg + computation.payload_removed_kg;
            state.fuel_remaining_kg -= computation.fuel_burn_kg;
            state.payload_remaining_kg -= computation.payload_removed_kg;
            state.altitude_m = computation.end_altitude_m;
            total_distance += computation.distance_m;
            total_duration += computation.duration_s;
            total_fuel += computation.fuel_burn_kg;
        }
        let completed = failed_segment.is_none();
        Ok(MissionResult {
            scenario_id: self.scenario.id.clone(),
            completed,
            total_distance: QuantityOutput::range(total_distance),
            total_duration_s: total_duration,
            total_fuel_burn_kg: total_fuel,
            reserve_fuel_remaining_kg: state.fuel_remaining_kg,
            initial_takeoff_mass_kg: initial_takeoff_mass,
            final_mass_kg: state.mass_kg,
            final_payload_mass_kg: state.payload_remaining_kg,
            failed_segment,
            fuel_exhausted,
            fuel_capacity_violation: capacity_exceeded,
            takeoff_mass_violation: initial_takeoff_mass > aircraft.mass.maximum_takeoff_mass_kg,
            segments: segment_results,
            assumptions: self.scenario.assumptions.clone(),
            warnings,
            model: mission_model(completed),
        })
    }

    fn compute_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        match segment.kind {
            SegmentKind::FixedFuel => self.fixed_fuel_segment(segment, state),
            SegmentKind::PayloadDrop => self.payload_drop_segment(segment, state),
            SegmentKind::Climb => self.climb_segment(segment, state),
            SegmentKind::Cruise => self.cruise_segment(segment, state),
            SegmentKind::Descent => self.descent_segment(segment, state),
            SegmentKind::Loiter | SegmentKind::Reserve => self.loiter_segment(segment, state),
            SegmentKind::StartAndTaxi
            | SegmentKind::FixedTime
            | SegmentKind::Takeoff
            | SegmentKind::Landing => self.timed_segment(segment, state),
        }
    }

    fn timed_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let duration = segment.duration_s.unwrap_or(0.0);
        let throttle = segment
            .power_fraction
            .or(segment.thrust_fraction)
            .unwrap_or(0.1);
        let speed = representative_speed(segment, &self.scenario, state.altitude_m)?;
        let mode = if segment.kind == SegmentKind::Takeoff {
            OperatingMode::Takeoff
        } else {
            OperatingMode::Economy
        };
        let fuel_flow = fuel::available(self, state.altitude_m, speed, throttle, mode)?;
        Ok(SegmentComputation {
            fuel_burn_kg: fuel_flow.flow_kg_s * duration,
            payload_removed_kg: 0.0,
            distance_m: if segment.kind == SegmentKind::Takeoff {
                speed * duration * 0.5
            } else {
                0.0
            },
            duration_s: duration,
            end_altitude_m: state.altitude_m,
            warnings: fuel_flow.warnings,
        })
    }

    fn fixed_fuel_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let fuel = segment
            .fuel_mass_kg
            .or_else(|| {
                segment
                    .fuel_fraction
                    .map(|fraction| state.mass_kg * (1.0 - fraction))
            })
            .ok_or_else(|| {
                AexError::validation(
                    "MISSING_FIXED_FUEL",
                    format!("mission.segments.{}", segment.id),
                    "fixed-fuel segment requires fuel mass or fraction",
                )
            })?;
        Ok(SegmentComputation {
            fuel_burn_kg: fuel,
            payload_removed_kg: 0.0,
            distance_m: 0.0,
            duration_s: 0.0,
            end_altitude_m: state.altitude_m,
            warnings: Vec::new(),
        })
    }

    fn payload_drop_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let payload = segment.payload_mass_kg.ok_or_else(|| {
            AexError::validation(
                "MISSING_PAYLOAD_MASS",
                format!("mission.segments.{}.payload_mass", segment.id),
                "payload-drop segment requires payload mass",
            )
        })?;
        if payload > state.payload_remaining_kg + 1.0e-8 {
            return Err(AexError::validation(
                "PAYLOAD_DROP_EXCEEDS_REMAINING",
                format!("mission.segments.{}.payload_mass", segment.id),
                format!(
                    "{payload} kg exceeds remaining payload {} kg",
                    state.payload_remaining_kg
                ),
            ));
        }
        Ok(SegmentComputation {
            fuel_burn_kg: 0.0,
            payload_removed_kg: payload,
            distance_m: 0.0,
            duration_s: 0.0,
            end_altitude_m: state.altitude_m,
            warnings: Vec::new(),
        })
    }

    fn climb_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let target = segment.target_altitude_m.ok_or_else(|| {
            AexError::validation(
                "MISSING_CLIMB_ALTITUDE",
                format!("mission.segments.{}.target_altitude", segment.id),
                "climb requires a target altitude",
            )
        })?;
        let midpoint = 0.5 * (state.altitude_m + target);
        let speed = representative_speed(segment, &self.scenario, midpoint)?;
        let analyzer = PointAnalyzer::new(self.scenario.clone());
        let climb = analyzer.maximum_rate_of_climb_with_diagnostics(midpoint, state.mass_kg)?;
        let throttle = segment
            .power_fraction
            .or(segment.thrust_fraction)
            .unwrap_or(0.9);
        let rate = (climb.maximum_rate_m_s * throttle).max(0.5);
        let duration = (target - state.altitude_m).max(0.0) / rate;
        let fuel_flow = fuel::available(self, midpoint, speed, throttle, OperatingMode::Climb)?;
        let mut warnings = climb.warnings;
        extend_unique_diagnostics(&mut warnings, fuel_flow.warnings);
        Ok(SegmentComputation {
            fuel_burn_kg: fuel_flow.flow_kg_s * duration,
            payload_removed_kg: 0.0,
            distance_m: speed * duration * 0.75,
            duration_s: duration,
            end_altitude_m: target,
            warnings,
        })
    }

    fn descent_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let target = segment.target_altitude_m.unwrap_or(0.0);
        let midpoint = 0.5 * (state.altitude_m + target);
        let speed = representative_speed(segment, &self.scenario, midpoint)?;
        let duration = (state.altitude_m - target).max(0.0) / 7.5;
        let throttle = segment
            .power_fraction
            .or(segment.thrust_fraction)
            .unwrap_or(0.15);
        let fuel_flow = fuel::available(self, midpoint, speed, throttle, OperatingMode::Economy)?;
        Ok(SegmentComputation {
            fuel_burn_kg: fuel_flow.flow_kg_s * duration,
            payload_removed_kg: 0.0,
            distance_m: speed * duration * 0.75,
            duration_s: duration,
            end_altitude_m: target,
            warnings: fuel_flow.warnings,
        })
    }

    fn cruise_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let distance = segment.distance_m.ok_or_else(|| {
            AexError::validation(
                "MISSING_SEGMENT_DISTANCE",
                format!("mission.segments.{}.distance", segment.id),
                "cruise requires distance",
            )
        })?;
        let altitude = segment.altitude_m.unwrap_or(state.altitude_m);
        let speed = representative_speed(segment, &self.scenario, altitude)?;
        let duration = distance / speed;
        let fuel = fuel::integrated(
            self,
            altitude,
            speed,
            state.mass_kg,
            duration,
            OperatingMode::Cruise,
        )?;
        Ok(SegmentComputation {
            fuel_burn_kg: fuel.fuel_kg,
            payload_removed_kg: 0.0,
            distance_m: distance,
            duration_s: duration,
            end_altitude_m: altitude,
            warnings: fuel.warnings,
        })
    }

    fn loiter_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let altitude = segment.altitude_m.unwrap_or(state.altitude_m);
        let duration = segment.duration_s.ok_or_else(|| {
            AexError::validation(
                "MISSING_SEGMENT_DURATION",
                format!("mission.segments.{}.duration", segment.id),
                "loiter requires duration",
            )
        })?;
        let speed = representative_speed(segment, &self.scenario, altitude)?;
        let fuel = fuel::integrated(
            self,
            altitude,
            speed,
            state.mass_kg,
            duration,
            OperatingMode::Economy,
        )?;
        Ok(SegmentComputation {
            fuel_burn_kg: fuel.fuel_kg,
            payload_removed_kg: 0.0,
            distance_m: 0.0,
            duration_s: duration,
            end_altitude_m: altitude,
            warnings: fuel.warnings,
        })
    }
}

fn mission_model(completed: bool) -> ModelMetadata {
    ModelMetadata {
        model_id: "mission.quasi_steady".to_owned(),
        model_version: "1.0.0".to_owned(),
        fidelity_level: 1,
        validity_status: if completed { "valid" } else { "incomplete" }.to_owned(),
    }
}

pub(crate) fn representative_speed(
    segment: &MissionSegment,
    scenario: &ResolvedScenario,
    altitude_m: f64,
) -> AexResult<f64> {
    if let Some(speed) = segment.true_airspeed_m_s {
        return Ok(speed);
    }
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    if let Some(mach) = segment.mach {
        return Ok(mach * atmosphere.speed_of_sound_m_s);
    }
    if let Some(indicated) = segment.indicated_airspeed_m_s {
        return Ok(indicated * (1.225 / atmosphere.density_kg_m3).sqrt());
    }
    match scenario.engine {
        EngineProfile::Piston(_) => Ok(45.0),
        EngineProfile::Turbofan(_) => Ok(120.0),
    }
}

fn initial_fuel_load(requested_kg: f64, capacity_kg: f64) -> InitialFuelLoad {
    let capacity_exceeded = requested_kg > capacity_kg + 1.0e-8;
    InitialFuelLoad {
        loaded_kg: requested_kg.min(capacity_kg),
        capacity_exceeded,
        warning: capacity_exceeded.then(|| {
            Diagnostic::warning(
                "FUEL_CAPACITY_EXCEEDED",
                format!(
                    "Requested initial fuel load {requested_kg} kg exceeds tank capacity \
                     {capacity_kg} kg."
                ),
                "mission.initial_fuel_load",
            )
        }),
    }
}

fn segment_result(
    segment: &MissionSegment,
    state: MissionState,
    computation: &SegmentComputation,
    warnings: Vec<Diagnostic>,
) -> MissionSegmentResult {
    MissionSegmentResult {
        segment_id: segment.id.clone(),
        start_mass_kg: state.mass_kg,
        end_mass_kg: state.mass_kg - computation.fuel_burn_kg - computation.payload_removed_kg,
        fuel_burn_kg: computation.fuel_burn_kg,
        payload_removed_kg: computation.payload_removed_kg,
        distance_m: computation.distance_m,
        duration_s: computation.duration_s,
        start_altitude_m: state.altitude_m,
        end_altitude_m: computation.end_altitude_m,
        warnings,
    }
}

fn extend_unique_diagnostics(target: &mut Vec<Diagnostic>, diagnostics: Vec<Diagnostic>) {
    for diagnostic in diagnostics {
        if !target
            .iter()
            .any(|existing| existing.code == diagnostic.code && existing.path == diagnostic.path)
        {
            target.push(diagnostic);
        }
    }
}

fn scoped_segment_warnings(segment: &MissionSegment, warnings: &[Diagnostic]) -> Vec<Diagnostic> {
    warnings
        .iter()
        .cloned()
        .map(|mut warning| {
            let base = format!("mission.segments.{}", segment.id);
            warning.path = warning
                .path
                .as_deref()
                .map_or_else(|| Some(base.clone()), |path| Some(format!("{base}.{path}")));
            warning
        })
        .collect()
}

#[cfg(test)]
mod tests;
