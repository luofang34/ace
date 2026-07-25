use crate::domain::diagnostic::{AexError, AexResult};

pub(super) fn bracket_roots<F>(
    start: f64,
    stop: f64,
    count: u32,
    function: F,
) -> AexResult<Vec<f64>>
where
    F: Fn(f64) -> AexResult<f64>,
{
    let mut roots = Vec::new();
    let mut left = start;
    let mut left_value = function(left)?;
    for index in 1..=count {
        let right = start + f64::from(index) * (stop - start) / f64::from(count);
        let right_value = function(right)?;
        if left_value * right_value <= 0.0 {
            roots.push(bounded_root(left, right, 1.0e-5, 80, &function)?);
        }
        left = right;
        left_value = right_value;
    }
    Ok(roots)
}

pub(super) fn bounded_root<F>(
    mut lower: f64,
    mut upper: f64,
    tolerance: f64,
    iterations: u32,
    function: F,
) -> AexResult<f64>
where
    F: Fn(f64) -> AexResult<f64>,
{
    let mut lower_value = function(lower)?;
    let upper_value = function(upper)?;
    if lower_value * upper_value > 0.0 {
        return Err(AexError::analysis(
            "ROOT_NOT_BRACKETED",
            format!("function has the same sign at {lower} and {upper}"),
        ));
    }
    for _ in 0..iterations {
        let midpoint = 0.5 * (lower + upper);
        let midpoint_value = function(midpoint)?;
        if (upper - lower).abs() <= tolerance {
            return Ok(midpoint);
        }
        if lower_value * midpoint_value <= 0.0 {
            upper = midpoint;
        } else {
            lower = midpoint;
            lower_value = midpoint_value;
        }
    }
    Err(AexError::analysis(
        "ROOT_NON_CONVERGENCE",
        format!("bounded solver did not converge in {iterations} iterations"),
    ))
}
