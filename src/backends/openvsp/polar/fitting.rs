use super::{AexResult, MACH, invalid};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Point {
    pub mach: f64,
    pub alpha: f64,
    pub cl: f64,
    pub cdi: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct Fit {
    pub mach: f64,
    pub k: f64,
    pub cl_limit: f64,
    pub maximum_absolute_error: f64,
    pub maximum_relative_error: f64,
}

pub(super) fn parse(raw: &str) -> AexResult<Vec<Point>> {
    raw.lines()
        .filter_map(|line| line.trim().strip_prefix("ACE_POLAR="))
        .map(|line| {
            let values = line
                .split(',')
                .map(|value| {
                    value
                        .trim()
                        .parse::<f64>()
                        .map_err(|error| invalid(format!("invalid polar row {line}: {error}")))
                })
                .collect::<AexResult<Vec<_>>>()?;
            let [mach, alpha, cl, cdi] = values.as_slice() else {
                return Err(invalid(format!("expected Mach, alpha, CL, CDi: {line}")));
            };
            if !values.iter().all(|value| value.is_finite()) {
                return Err(invalid("nonfinite polar sample"));
            }
            Ok(Point {
                mach: *mach,
                alpha: *alpha,
                cl: *cl,
                cdi: *cdi,
            })
        })
        .collect()
}

pub(super) fn fit(points: &[Point]) -> AexResult<Vec<Fit>> {
    if points.len() != 18 {
        return Err(invalid(
            "requires exactly six alpha samples at each of three Mach points",
        ));
    }
    MACH.into_iter()
        .map(|mach| fit_slice(points, mach))
        .collect()
}

fn fit_slice(points: &[Point], mach: f64) -> AexResult<Fit> {
    let mut samples: Vec<_> = points
        .iter()
        .filter(|p| (p.mach - mach).abs() < 1e-8)
        .collect();
    samples.sort_by(|a, b| a.alpha.total_cmp(&b.alpha));
    if samples.len() != 6
        || samples
            .iter()
            .zip([0., 2., 4., 6., 8., 10.])
            .any(|(p, alpha)| (p.alpha - alpha).abs() > 1e-8 || p.cdi < 0.0)
        || samples.windows(2).any(|pair| {
            pair.first()
                .zip(pair.last())
                .is_none_or(|(a, b)| b.cl <= a.cl)
        })
    {
        return Err(invalid(format!(
            "incomplete or nonmonotonic polar at Mach {mach}"
        )));
    }
    let first = samples.first().ok_or_else(|| invalid("empty polar"))?;
    if first.cl.abs() > 1e-3 || first.cdi.abs() > 2e-5 {
        return Err(invalid(
            "zero-lift symmetry check failed; quadratic reduction requires an untwisted symmetric wing",
        ));
    }
    let numerator: f64 = samples.iter().map(|p| p.cl.powi(2) * p.cdi).sum();
    let denominator: f64 = samples.iter().map(|p| p.cl.powi(4)).sum();
    let k = numerator / denominator;
    if !k.is_finite() || k <= 0.0 {
        return Err(invalid("nonpositive or nonfinite induced-drag factor"));
    }
    let mut absolute: f64 = 0.0;
    let mut relative: f64 = 0.0;
    for point in &samples {
        let error = (point.cdi - k * point.cl.powi(2)).abs();
        if error > 2e-5 + 0.05 * point.cdi {
            return Err(invalid(format!(
                "quadratic reduction exceeds 5% + 0.00002 CDi at Mach {mach}, alpha {}",
                point.alpha
            )));
        }
        absolute = absolute.max(error);
        relative = relative.max(error / point.cdi.max(2e-5));
    }
    Ok(Fit {
        mach,
        k,
        cl_limit: samples.last().ok_or_else(|| invalid("empty polar"))?.cl,
        maximum_absolute_error: absolute,
        maximum_relative_error: relative,
    })
}

#[cfg(test)]
mod tests;
