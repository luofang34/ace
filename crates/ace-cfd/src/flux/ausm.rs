use super::{Conserved, IdealGas, Primitive, Vector, dot};

// Liou, JCP 214 (2006), doi:10.1016/j.jcp.2005.09.020.
pub(crate) fn ausm(
    gas: IdealGas,
    left: Primitive,
    right: Primitive,
    n: Vector,
    reference_mach: f64,
) -> Conserved {
    let ul = dot(left.velocity, n);
    let ur = dot(right.velocity, n);
    let hl = (gas.conservative(left)[4] + left.pressure) / left.density;
    let hr = (gas.conservative(right)[4] + right.pressure) / right.density;
    let critical_l = 2.0 * (gas.gamma - 1.0) * hl / (gas.gamma + 1.0);
    let critical_r = 2.0 * (gas.gamma - 1.0) * hr / (gas.gamma + 1.0);
    let sound =
        (critical_l / critical_l.sqrt().max(ul)).min(critical_r / critical_r.sqrt().max(-ur));
    let ml = ul / sound;
    let mr = ur / sound;
    let mean_mach_squared = 0.5 * (ml * ml + mr * mr);
    let m0 = mean_mach_squared
        .max(reference_mach.powi(2))
        .clamp(1.0e-4, 1.0)
        .sqrt();
    let fa = m0 * (2.0 - m0);
    let alpha = 3.0 / 16.0 * (-4.0 + 5.0 * fa * fa);
    let pl = pressure_split(ml, 1.0, alpha);
    let pr = pressure_split(mr, -1.0, alpha);
    let density = 0.5 * (left.density + right.density);
    let mach = mach_split(ml, 1.0) + mach_split(mr, -1.0)
        - 0.25 / fa * (1.0 - mean_mach_squared).max(0.0) * (right.pressure - left.pressure)
            / (density * sound * sound);
    let pressure = pl * left.pressure + pr * right.pressure
        - 0.75 * pl * pr * (left.density + right.density) * fa * sound * (ur - ul);
    let (q, h) = if mach >= 0.0 { (left, hl) } else { (right, hr) };
    let mass = sound * mach * q.density;
    [
        mass,
        mass * q.velocity[0] + pressure * n[0],
        mass * q.velocity[1] + pressure * n[1],
        mass * q.velocity[2] + pressure * n[2],
        mass * h,
    ]
}

fn mach_split(m: f64, sign: f64) -> f64 {
    if m.abs() >= 1.0 {
        return 0.5 * (m + sign * m.abs());
    }
    sign * 0.25 * (m + sign).powi(2) + sign * 0.125 * (m * m - 1.0).powi(2)
}

fn pressure_split(m: f64, sign: f64, alpha: f64) -> f64 {
    if m.abs() >= 1.0 {
        return 0.5 * (1.0 + sign * m.signum());
    }
    0.25 * (m + sign).powi(2) * (2.0 - sign * m) + sign * alpha * m * (m * m - 1.0).powi(2)
}
