use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::result::AtmosphereState;
use crate::domain::schema::{MissionSegment, ResolvedScenario};
use crate::models::atmosphere::Isa1976;
use crate::models::mission::representative_speed;

#[derive(Debug, Clone, Copy)]
pub(super) struct EffectiveSpeed {
    pub(super) source_field: &'static str,
    pub(super) true_airspeed_m_s: Option<f64>,
    pub(super) mach: Option<f64>,
}

pub(super) fn effective_speed(
    altitude_m: f64,
    true_airspeed_m_s: Option<f64>,
    mach: Option<f64>,
    indicated_airspeed_m_s: Option<f64>,
) -> AexResult<Option<EffectiveSpeed>> {
    let atmosphere = atmosphere_for_preflight(altitude_m)?;
    if let Some(speed) = true_airspeed_m_s {
        return Ok(Some(EffectiveSpeed {
            source_field: "true_airspeed",
            true_airspeed_m_s: Some(speed),
            mach: atmosphere.map(|state| speed / state.speed_of_sound_m_s),
        }));
    }
    if let Some(mach) = mach {
        return Ok(Some(EffectiveSpeed {
            source_field: "mach",
            true_airspeed_m_s: atmosphere.map(|state| mach * state.speed_of_sound_m_s),
            mach: Some(mach),
        }));
    }
    Ok(indicated_airspeed_m_s.map(|speed| {
        let true_airspeed_m_s = atmosphere
            .as_ref()
            .map_or(speed, |state| speed * (1.225 / state.density_kg_m3).sqrt());
        EffectiveSpeed {
            source_field: "indicated_airspeed",
            true_airspeed_m_s: Some(true_airspeed_m_s),
            mach: atmosphere
                .as_ref()
                .map(|state| true_airspeed_m_s / state.speed_of_sound_m_s),
        }
    }))
}

pub(super) fn effective_segment_speed(
    segment: &MissionSegment,
    scenario: &ResolvedScenario,
    altitude_m: f64,
) -> AexResult<Option<EffectiveSpeed>> {
    let source_field = if segment.true_airspeed_m_s.is_some() {
        "true_airspeed"
    } else if segment.mach.is_some() {
        "mach"
    } else if segment.indicated_airspeed_m_s.is_some() {
        "indicated_airspeed"
    } else {
        return Ok(None);
    };
    let Some(atmosphere) = atmosphere_for_preflight(altitude_m)? else {
        return effective_speed(
            altitude_m,
            segment.true_airspeed_m_s,
            segment.mach,
            segment.indicated_airspeed_m_s,
        );
    };
    let true_airspeed_m_s = representative_speed(segment, scenario, altitude_m)?;
    let mach = if source_field == "mach" {
        segment.mach
    } else {
        Some(true_airspeed_m_s / atmosphere.speed_of_sound_m_s)
    };
    Ok(Some(EffectiveSpeed {
        source_field,
        true_airspeed_m_s: Some(true_airspeed_m_s),
        mach,
    }))
}

fn atmosphere_for_preflight(altitude_m: f64) -> AexResult<Option<AtmosphereState>> {
    match Isa1976::new(0.0).evaluate(altitude_m) {
        Ok(atmosphere) => Ok(Some(atmosphere)),
        Err(AexError::Analysis {
            code: "ATMOSPHERE_OUTSIDE_VALIDITY",
            ..
        }) => Ok(None),
        Err(error) => Err(error),
    }
}
