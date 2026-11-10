use std::f64::consts::PI;

use ace_cfd::{Solution, WallSample};
use glam::DVec3;
use mbd_core::{BluntBodyCapsuleAero, BluntBodyCapsuleParams};
use syspack::DynamicsSpec;

use crate::AceExportError;

#[cfg(test)]
mod tests;

/// The free stream a blunt-body solution was run at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapsuleFreestream {
    /// Density, kg/m³.
    pub density_kg_m3: f64,
    /// Speed, m/s.
    pub speed_m_s: f64,
    /// Static pressure, Pa.
    pub pressure_pa: f64,
}

/// What a solution over a sphere cannot say about the capsule it stands for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapsuleGeometry {
    /// Maximum radius of the heat shield, metres.
    pub base_radius_m: f64,
    /// Spherical radius of the heat shield, metres.
    pub nose_radius_m: f64,
    /// Heat-shield apex along body `+x`, metres: the sphere's centre lies
    /// `nose_radius_m` behind it.
    pub apex_x_body_m: f64,
    /// `Cmq`, which an axisymmetric steady solution has no way to supply.
    pub pitch_damping_coeff: f64,
}

/// Why `value` cannot normalise anything.
fn positive(field: &'static str, value: f64) -> Result<f64, AceExportError> {
    if value.is_finite() && value > 0.0 {
        return Ok(value);
    }
    Err(AceExportError::InvalidFreestream { field, value })
}

fn pressure_coefficient(sample: &WallSample, stream: &CapsuleFreestream, q_pa: f64) -> f64 {
    (sample.pressure - stream.pressure_pa) / q_pa
}

/// The capsule coefficients of a solved wall.
///
/// The solver's sphere may be any size: pressure coefficient depends on the
/// angle from the axis alone, so its distribution is laid on a heat shield of
/// `nose_radius_m` and integrated out to the rim, with the wall face straddling
/// the rim counted whole or not at all by its centroid. Heating scales as
/// `√(ρ/R)` from the radius it was solved at.
///
/// An axisymmetric solution has no incidence, so `normal_force_slope` comes
/// from the modified-Newtonian result for a spherical cap of half-angle `θ`,
/// `s = Cp,max sin²θ / 2`.
pub fn capsule_params_from_wall(
    wall: &[WallSample],
    stream: &CapsuleFreestream,
    geometry: &CapsuleGeometry,
) -> Result<BluntBodyCapsuleParams, AceExportError> {
    let shape = BluntBodyCapsuleParams {
        base_radius_m: geometry.base_radius_m,
        nose_radius_m: geometry.nose_radius_m,
        reference_point_body_m: DVec3::X * (geometry.apex_x_body_m - geometry.nose_radius_m),
        normal_force_slope: 0.0,
        axial_drag_coeff: 0.0,
        pitch_damping_coeff: geometry.pitch_damping_coeff,
        sutton_graves_k: 0.0,
    };
    BluntBodyCapsuleAero::new(shape).map_err(AceExportError::InvalidCapsule)?;
    let density = positive("density_kg_m3", stream.density_kg_m3)?;
    let speed = positive("speed_m_s", stream.speed_m_s)?;
    positive("pressure_pa", stream.pressure_pa)?;
    let q_pa = 0.5 * density * speed * speed;
    let stagnation = wall
        .iter()
        .min_by(|a, b| a.theta.total_cmp(&b.theta))
        .ok_or(AceExportError::InvalidWall {
            detail: "no samples",
        })?;
    let [x, y, z] = stagnation.position;
    let solved_radius_m = DVec3::new(x, y, z).length();
    let CapsuleGeometry {
        base_radius_m,
        nose_radius_m,
        ..
    } = *geometry;
    let rim_sine = base_radius_m / nose_radius_m;
    let rim_angle = rim_sine.asin();
    let cap: Vec<&WallSample> = wall.iter().filter(|s| s.theta <= rim_angle).collect();
    let reaches_rim = wall.iter().any(|s| s.theta >= rim_angle);
    if !reaches_rim || cap.is_empty() {
        return Err(AceExportError::InvalidWall {
            detail: "the samples do not reach the heat shield's rim",
        });
    }
    let area_scale = (nose_radius_m / solved_radius_m).powi(2);
    let axial_force_coeff: f64 = cap
        .iter()
        .map(|s| pressure_coefficient(s, stream, q_pa) * s.theta.cos() * s.area * area_scale)
        .sum::<f64>()
        / (PI * base_radius_m * base_radius_m);
    let peak_cp = pressure_coefficient(stagnation, stream, q_pa);
    let heat_scale = (density / solved_radius_m).sqrt() * speed.powi(3);
    let params = BluntBodyCapsuleParams {
        normal_force_slope: peak_cp * rim_sine * rim_sine / 2.0,
        axial_drag_coeff: axial_force_coeff,
        sutton_graves_k: stagnation.heat_flux / heat_scale,
        ..shape
    };
    BluntBodyCapsuleAero::new(params).map_err(AceExportError::InvalidCapsule)?;
    Ok(params)
}

/// The manifest dynamics of a converged `ace-cfd` solution. An unconverged one
/// is refused, whatever its wall loads look like.
pub fn capsule_dynamics_from_solution(
    solution: &Solution,
    stream: &CapsuleFreestream,
    geometry: &CapsuleGeometry,
) -> Result<DynamicsSpec, AceExportError> {
    if !solution.converged {
        return Err(AceExportError::SolutionNotConverged {
            steps: solution.steps,
        });
    }
    let params = capsule_params_from_wall(&solution.wall, stream, geometry)?;
    Ok(DynamicsSpec::RigidBodyBluntCapsule {
        base_radius_m: params.base_radius_m,
        nose_radius_m: params.nose_radius_m,
        reference_point_body_m: params.reference_point_body_m.to_array(),
        normal_force_slope: params.normal_force_slope,
        axial_drag_coeff: params.axial_drag_coeff,
        pitch_damping_coeff: params.pitch_damping_coeff,
        sutton_graves_k: params.sutton_graves_k,
    })
}
