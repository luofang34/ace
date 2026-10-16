use super::{Conserved, IdealGas, Primitive, Vector, dot};

// Wada and Liou, NASA TM-106452, equations 29–40 and the sonic entropy correction.
pub(crate) fn ausmdv(
    gas: IdealGas,
    left: Primitive,
    right: Primitive,
    n: Vector,
    shock: f64,
) -> Conserved {
    let ul = dot(left.velocity, n);
    let ur = dot(right.velocity, n);
    let cl = gas.sound_speed(left);
    let cr = gas.sound_speed(right);
    let c = cl.max(cr);
    let tl = left.pressure / left.density;
    let tr = right.pressure / right.density;
    let vl = velocity_split(ul, c, 1.0, 2.0 * tl / (tl + tr));
    let vr = velocity_split(ur, c, -1.0, 2.0 * tr / (tl + tr));
    let pressure =
        pressure_split(ul / c, 1.0) * left.pressure + pressure_split(ur / c, -1.0) * right.pressure;
    let mass = vl * left.density + vr * right.density;
    let upwind = if mass >= 0.0 { left } else { right };
    let mut flux = advected(gas, upwind).map(|v| mass * v / upwind.density);
    let switch = 0.5
        * (10.0 * (right.pressure - left.pressure).abs() / left.pressure.min(right.pressure))
            .min(1.0);
    let momentum_v = vl * left.density * ul + vr * right.density * ur;
    let correction = (0.5 + switch) * (momentum_v - mass * dot(upwind.velocity, n));
    for k in 0..3 {
        flux[k + 1] += (pressure + correction) * n[k];
    }
    let minus = ul - cl < 0.0 && ur - cr > 0.0;
    let plus = ul + cl < 0.0 && ur + cr > 0.0;
    if minus != plus {
        let speed = if minus {
            ur - cr - ul + cl
        } else {
            ur + cr - ul - cl
        };
        let a = advected(gas, left);
        let b = advected(gas, right);
        for k in 0..5 {
            flux[k] -= 0.125 * speed * (b[k] - a[k]);
        }
    }
    let dissipative = hanel(gas, left, right, n);
    std::array::from_fn(|k| (1.0 - shock) * flux[k] + shock * dissipative[k])
}

fn advected(gas: IdealGas, q: Primitive) -> Conserved {
    let mut u = gas.conservative(q);
    u[4] += q.pressure;
    u
}

fn hanel(gas: IdealGas, left: Primitive, right: Primitive, n: Vector) -> Conserved {
    let mut result = [0.0; 5];
    for (q, sign) in [(left, 1.0), (right, -1.0)] {
        let c = gas.sound_speed(q);
        let un = dot(q.velocity, n);
        let v = velocity_split(un, c, sign, 1.0);
        let pressure = q.pressure * pressure_split(un / c, sign);
        let a = advected(gas, q);
        for k in 0..5 {
            result[k] += v * a[k];
        }
        for k in 0..3 {
            result[k + 1] += pressure * n[k];
        }
    }
    result
}

fn velocity_split(u: f64, c: f64, sign: f64, alpha: f64) -> f64 {
    let upwind = 0.5 * (u + sign * u.abs());
    if u.abs() >= c {
        return upwind;
    }
    alpha * (sign * (u + sign * c).powi(2) / (4.0 * c) - upwind) + upwind
}

fn pressure_split(m: f64, sign: f64) -> f64 {
    if m.abs() >= 1.0 {
        return 0.5 * (1.0 + sign * m.signum());
    }
    0.25 * (m + sign).powi(2) * (2.0 - sign * m)
}
