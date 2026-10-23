//! Canonical rotation matrices expressed as OpenVSP `XForm` angles.

use ace_engine::SolverError;
use ace_physics::Frame;

const TOLERANCE: f64 = 1e-9;

/// OpenVSP composes its three angles as Rx·Ry·Rz, applying the z rotation first; the order
/// was established from exported tessellations of rotated surfaces.
pub(super) fn rotation_deg(r: [[f64; 3]; 3]) -> [f64; 3] {
    let sin_y = r[0][2].clamp(-1.0, 1.0);
    let cos_y = (1.0 - sin_y * sin_y).sqrt();
    let (x, z) = if cos_y > TOLERANCE {
        ((-r[1][2]).atan2(r[2][2]), (-r[0][1]).atan2(r[0][0]))
    } else if sin_y > 0.0 {
        (r[1][0].atan2(r[1][1]), 0.0)
    } else {
        ((-r[1][0]).atan2(r[1][1]), 0.0)
    };
    [x.to_degrees(), sin_y.asin().to_degrees(), z.to_degrees()]
}

/// The kernel mirrors across the vehicle x/z plane through the object origin, so a symmetric
/// surface must keep its local y axis parallel to the vehicle y axis at zero lateral offset.
pub(super) fn check_symmetric(id: &str, frame: &Frame) -> Result<(), SolverError> {
    let lateral = [
        frame.rotation[0][1],
        frame.rotation[1][1],
        frame.rotation[2][1],
    ];
    if lateral[0].abs() > TOLERANCE
        || lateral[2].abs() > TOLERANCE
        || frame.translation[1].abs() > TOLERANCE
    {
        return Err(SolverError::Failed(format!(
            "{id}: symmetric surfaces must keep their local y axis parallel to the vehicle y axis at zero lateral offset"
        )));
    }
    Ok(())
}
