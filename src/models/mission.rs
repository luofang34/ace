use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::MissionResult;
use crate::domain::schema::{MissionSegment, ResolvedScenario, SegmentKind};
use crate::domain::warning::WarningCode;
use crate::models::atmosphere::Isa1976;
use crate::models::performance::PointAnalyzer;
use crate::models::propulsion::OperatingMode;

mod energy_climb;
mod fuel;
mod initial_state;
mod landing_fuel;
mod operating_condition;
mod record;

#[cfg(test)]
use initial_state::initial_fuel_load;
use initial_state::{InitialMissionState, MissionState, initial_mission_state};
use operating_condition::representative_speed_with_fallback;
pub(crate) use operating_condition::{
    energy_schedule_speed, representative_speed, segment_end_altitude, segment_operating_altitude,
};
use record::{extend_unique_diagnostics, mission_model, scoped_segment_warnings, segment_result};

#[derive(Debug, Clone)]
pub(crate) struct MissionSimulator {
    scenario: ResolvedScenario,
    atmosphere: Isa1976,
}

#[derive(Debug, Clone)]
struct SegmentComputation {
    fuel_burn_kg: f64,
    payload_removed_kg: f64,
    distance_m: f64,
    duration_s: f64,
    end_altitude_m: f64,
    end_speed_m_s: Option<f64>,
    operating_speed_m_s: Option<f64>,
    warnings: Vec<Diagnostic>,
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
        let InitialMissionState {
            mut state,
            capacity_exceeded,
            warning: fuel_load_warning,
        } = initial_mission_state(&self.scenario)?;
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
                    WarningCode::FuelExhausted,
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
            state.speed_m_s = computation.end_speed_m_s;
            total_distance += computation.distance_m;
            total_duration += computation.duration_s;
            total_fuel += computation.fuel_burn_kg;
        }
        let completed = failed_segment.is_none();
        let landing_fuel = landing_fuel::evaluate(
            completed,
            state.fuel_remaining_kg,
            aircraft.mass.maximum_fuel_mass_kg,
        );
        warnings.extend(landing_fuel.warning);
        Ok(MissionResult {
            scenario_id: self.scenario.id.clone(),
            completed,
            total_distance: QuantityOutput::range(total_distance),
            total_duration_s: total_duration,
            total_fuel_burn_kg: total_fuel,
            reserve_fuel_remaining_kg: state.fuel_remaining_kg,
            landing_fuel: landing_fuel.value,
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
            SegmentKind::EnergyClimb => energy_climb::compute(self, segment, state),
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
        let altitude = segment_operating_altitude(segment, state.altitude_m);
        let speed =
            representative_speed_with_fallback(segment, &self.scenario, altitude, state.speed_m_s)?;
        let mode = if segment.kind == SegmentKind::Takeoff {
            OperatingMode::Takeoff
        } else {
            OperatingMode::Economy
        };
        let fuel_flow = fuel::available(self, altitude, speed, throttle, mode)?;
        Ok(SegmentComputation {
            fuel_burn_kg: fuel_flow.flow_kg_s * duration,
            payload_removed_kg: 0.0,
            distance_m: if segment.kind == SegmentKind::Takeoff {
                speed * duration * 0.5
            } else {
                0.0
            },
            duration_s: duration,
            end_altitude_m: segment_end_altitude(segment, state.altitude_m),
            end_speed_m_s: state.speed_m_s.map(|_| speed),
            operating_speed_m_s: Some(speed),
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
            end_speed_m_s: state.speed_m_s,
            operating_speed_m_s: None,
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
            end_speed_m_s: state.speed_m_s,
            operating_speed_m_s: None,
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
        if segment_engine_off(segment) && target > state.altitude_m {
            return Err(AexError::validation(
                "ENGINE_OFF_CLIMB_UNSUPPORTED",
                format!("mission.segments.{}", segment.id),
                "engine-off climb cannot increase altitude",
            ));
        }
        let midpoint = segment_operating_altitude(segment, state.altitude_m);
        let speed =
            representative_speed_with_fallback(segment, &self.scenario, midpoint, state.speed_m_s)?;
        let analyzer = PointAnalyzer::new(self.scenario.clone());
        let climb = analyzer.maximum_rate_of_climb_with_diagnostics(midpoint, state.mass_kg)?;
        let throttle = segment
            .power_fraction
            .or(segment.thrust_fraction)
            .unwrap_or(0.9);
        let uncapped_rate = climb.maximum_rate_m_s * throttle;
        let rate = uncapped_rate.clamp(0.5, 50.0);
        let duration = (target - state.altitude_m).max(0.0) / rate;
        let fuel_flow = fuel::available(self, midpoint, speed, throttle, OperatingMode::Climb)?;
        let mut warnings = climb.warnings;
        if uncapped_rate > 50.0 {
            warnings.push(Diagnostic::limitation(
                "Legacy climb rate is capped at 50 m/s; use energy_climb for acceleration.",
            ));
        }
        extend_unique_diagnostics(&mut warnings, fuel_flow.warnings);
        Ok(SegmentComputation {
            fuel_burn_kg: fuel_flow.flow_kg_s * duration,
            payload_removed_kg: 0.0,
            distance_m: speed * duration * 0.75,
            duration_s: duration,
            end_altitude_m: target,
            end_speed_m_s: state.speed_m_s.map(|_| speed),
            operating_speed_m_s: Some(speed),
            warnings,
        })
    }

    fn descent_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let target = segment.target_altitude_m.unwrap_or(0.0);
        let midpoint = segment_operating_altitude(segment, state.altitude_m);
        let speed =
            representative_speed_with_fallback(segment, &self.scenario, midpoint, state.speed_m_s)?;
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
            end_speed_m_s: state.speed_m_s.map(|_| speed),
            operating_speed_m_s: Some(speed),
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
        let altitude = segment_operating_altitude(segment, state.altitude_m);
        let speed =
            representative_speed_with_fallback(segment, &self.scenario, altitude, state.speed_m_s)?;
        let duration = distance / speed;
        let fuel = fuel::integrated(
            self,
            altitude,
            speed,
            state.mass_kg,
            duration,
            OperatingMode::Cruise,
            segment_engine_off(segment),
        )?;
        Ok(SegmentComputation {
            fuel_burn_kg: fuel.fuel_kg,
            payload_removed_kg: 0.0,
            distance_m: distance,
            duration_s: duration,
            end_altitude_m: altitude,
            end_speed_m_s: state.speed_m_s.map(|_| speed),
            operating_speed_m_s: Some(speed),
            warnings: fuel.warnings,
        })
    }

    fn loiter_segment(
        &self,
        segment: &MissionSegment,
        state: MissionState,
    ) -> AexResult<SegmentComputation> {
        let altitude = segment_operating_altitude(segment, state.altitude_m);
        let duration = segment.duration_s.ok_or_else(|| {
            AexError::validation(
                "MISSING_SEGMENT_DURATION",
                format!("mission.segments.{}.duration", segment.id),
                "loiter requires duration",
            )
        })?;
        let speed =
            representative_speed_with_fallback(segment, &self.scenario, altitude, state.speed_m_s)?;
        let fuel = fuel::integrated(
            self,
            altitude,
            speed,
            state.mass_kg,
            duration,
            OperatingMode::Economy,
            segment_engine_off(segment),
        )?;
        Ok(SegmentComputation {
            fuel_burn_kg: fuel.fuel_kg,
            payload_removed_kg: 0.0,
            distance_m: 0.0,
            duration_s: duration,
            end_altitude_m: altitude,
            end_speed_m_s: state.speed_m_s.map(|_| speed),
            operating_speed_m_s: Some(speed),
            warnings: fuel.warnings,
        })
    }
}

pub(crate) fn segment_engine_off(segment: &MissionSegment) -> bool {
    segment.power_fraction == Some(0.0) || segment.thrust_fraction == Some(0.0)
}

#[cfg(test)]
mod tests;
