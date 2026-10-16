use crate::gas::{Conserved, IdealGas, Primitive};
use crate::vector::{Vector, dot};
use serde::{Deserialize, Serialize};

mod ausm;
pub(crate) use ausm::ausm;
mod ausmdv;
pub(crate) use ausmdv::ausmdv;

/// Convective flux selected independently of thermodynamics and mesh topology.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Convection {
    /// Contact resolution with pressure-sensor shock dissipation.
    #[default]
    HybridHllc,
    /// AUSM+-up pressure and advection splitting.
    AusmPlusUp,
    /// AUSMDV with enthalpy-preserving shock dissipation.
    Ausmdv,
    /// AUSMDV without additional shock blending, for flux sensitivity verification.
    PureAusmdv,
}

pub(crate) fn physical(gas: IdealGas, q: Primitive, n: Vector) -> Conserved {
    let un = dot(q.velocity, n);
    let u = gas.conservative(q);
    [
        u[0] * un,
        u[1] * un + q.pressure * n[0],
        u[2] * un + q.pressure * n[1],
        u[3] * un + q.pressure * n[2],
        (u[4] + q.pressure) * un,
    ]
}

// HLLE avoids a contact wave's transverse shock instability on the stagnation line.
pub(crate) fn hlle(gas: IdealGas, left: Primitive, right: Primitive, n: Vector) -> Conserved {
    let (sl, sr) = signal_speeds(gas, left, right, n);
    let (sl, sr) = (sl.min(0.0), sr.max(0.0));
    let fl = physical(gas, left, n);
    let fr = physical(gas, right, n);
    let ql = gas.conservative(left);
    let qr = gas.conservative(right);
    std::array::from_fn(|i| (sr * fl[i] - sl * fr[i] + sl * sr * (qr[i] - ql[i])) / (sr - sl))
}

pub(crate) fn hllc(gas: IdealGas, left: Primitive, right: Primitive, n: Vector) -> Conserved {
    let ul = dot(left.velocity, n);
    let ur = dot(right.velocity, n);
    let (sl, sr) = signal_speeds(gas, left, right, n);
    if sl >= 0.0 {
        return physical(gas, left, n);
    }
    if sr <= 0.0 {
        return physical(gas, right, n);
    }
    let dl = left.density * (sl - ul);
    let dr = right.density * (sr - ur);
    let contact = (right.pressure - left.pressure + dl * ul - dr * ur) / (dl - dr);
    let pressure = left.pressure + dl * (contact - ul);
    if pressure <= 0.0 || !pressure.is_finite() {
        return hlle(gas, left, right, n);
    }
    let (q, speed, un) = if contact >= 0.0 {
        (left, sl, ul)
    } else {
        (right, sr, ur)
    };
    let u = gas.conservative(q);
    let rho = q.density * (speed - un) / (speed - contact);
    let velocity: Vector = std::array::from_fn(|i| q.velocity[i] + (contact - un) * n[i]);
    let energy = rho
        * (u[4] / q.density + (contact - un) * (contact + q.pressure / (q.density * (speed - un))));
    let star = [
        rho,
        rho * velocity[0],
        rho * velocity[1],
        rho * velocity[2],
        energy,
    ];
    let flux = physical(gas, q, n);
    std::array::from_fn(|k| flux[k] + speed * (star[k] - u[k]))
}

// Roe-averaged Einfeldt bounds preserve an isolated stationary shock.
fn signal_speeds(gas: IdealGas, left: Primitive, right: Primitive, n: Vector) -> (f64, f64) {
    let l = left.density.sqrt();
    let r = right.density.sqrt();
    let velocity =
        std::array::from_fn(|k| (l * left.velocity[k] + r * right.velocity[k]) / (l + r));
    let enthalpy = |q: Primitive| (gas.conservative(q)[4] + q.pressure) / q.density;
    let h = (l * enthalpy(left) + r * enthalpy(right)) / (l + r);
    let sound = ((gas.gamma - 1.0) * (h - 0.5 * dot(velocity, velocity))).sqrt();
    let un = dot(velocity, n);
    (
        (dot(left.velocity, n) - gas.sound_speed(left)).min(un - sound),
        (dot(right.velocity, n) + gas.sound_speed(right)).max(un + sound),
    )
}

pub(crate) fn viscous(
    gas: IdealGas,
    velocity: Vector,
    gradient: [Vector; 4],
    n: Vector,
    radial_strain: f64,
) -> Conserved {
    let divergence = gradient[0][0] + gradient[1][1] + gradient[2][2] + radial_strain;
    let traction: Vector = std::array::from_fn(|i| {
        (0..3)
            .map(|j| {
                let diagonal = if i == j { 2.0 * divergence / 3.0 } else { 0.0 };
                gas.viscosity * (gradient[i][j] + gradient[j][i] - diagonal) * n[j]
            })
            .sum()
    });
    [
        0.0,
        traction[0],
        traction[1],
        traction[2],
        dot(traction, velocity) + gas.conductivity * dot(gradient[3], n),
    ]
}

#[cfg(test)]
mod tests;
