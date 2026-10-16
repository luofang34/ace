use crate::vector::norm;
use crate::{CfdError, Conserved, FlowOperator, WallSample};
use serde::{Deserialize, Serialize};

/// Failure of the flow calculation or a caller-owned observation sink.
#[derive(Debug, thiserror::Error)]
pub enum ObservationError<E: std::error::Error + 'static> {
    /// A physical or numerical precondition failed.
    #[error(transparent)]
    Flow(#[from] CfdError),
    /// The observer could not record the accepted iteration state.
    #[error("solver observation failed: {0}")]
    Observer(#[source] E),
}

/// Explicit local pseudo-time iteration controls for steady flows only.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolverConfig {
    /// Courant factor, at most 0.8 for this explicit implementation.
    pub cfl: f64,
    /// Maximum number of two-stage iterations.
    pub max_steps: usize,
    /// Iterations with first-order convection before reconstruction is enabled.
    pub first_order_steps: usize,
    /// Interval for residual and heat-flux checks.
    pub check_interval: usize,
    /// Dimensionless RMS residual, normalized by free-stream scales and R/U.
    pub residual_tolerance: f64,
    /// Maximum relative wall heat-flux change between checks.
    pub heat_flux_tolerance: f64,
}

/// A measured convergence check; residuals are not multiplied by local time steps.
#[derive(Debug, Clone, Serialize)]
pub struct ConvergenceSample {
    /// Completed pseudo-time iterations.
    pub step: usize,
    /// Dimensionless RMS residual over every equation and cell.
    pub residual: f64,
    /// Cell with the largest dimensionless residual component.
    pub peak_cell: usize,
    /// Maximum heat-flux change, normalized by peak absolute wall heat flux.
    pub heat_flux_change: f64,
    /// Heat flux in the first wall face adjacent to the stagnation axis (W/m²).
    pub nose_heat_flux: f64,
    /// Integral of the signed wall heat flux over the full revolved wall (W).
    pub integrated_heating: f64,
}

/// A steady-solver result; unconverged states remain explicitly distinguishable.
#[derive(Debug, Serialize)]
pub struct Solution {
    /// True only after consecutive residual and thermal checks pass.
    pub converged: bool,
    /// Completed iteration count.
    pub steps: usize,
    /// Conservative cell states in mesh order.
    pub state: Vec<Conserved>,
    /// Wall pressure and heat-flux distribution.
    pub wall: Vec<WallSample>,
    /// Recorded residual and wall convergence evidence.
    pub history: Vec<ConvergenceSample>,
}

/// Solve the steady equations with local pseudo-time SSP-RK2 and no state clipping.
pub fn solve(
    op: &FlowOperator,
    config: SolverConfig,
    initial: Option<Vec<Conserved>>,
) -> Result<Solution, CfdError> {
    solve_observed(op, config, initial, |_| {})
}

/// Solve while delivering measured convergence checks to a caller-owned observer.
pub fn solve_observed(
    op: &FlowOperator,
    config: SolverConfig,
    initial: Option<Vec<Conserved>>,
    mut observer: impl FnMut(&ConvergenceSample),
) -> Result<Solution, CfdError> {
    match solve_checkpointed(op, config, initial, |sample, _| {
        observer(sample);
        Ok::<_, std::convert::Infallible>(())
    }) {
        Ok(solution) => Ok(solution),
        Err(ObservationError::Flow(error)) => Err(error),
        Err(ObservationError::Observer(error)) => match error {},
    }
}

/// Observe accepted states at convergence checks without changing the iteration.
/// An observer error stops the solve and retains its original error source.
pub fn solve_checkpointed<E: std::error::Error + 'static>(
    op: &FlowOperator,
    config: SolverConfig,
    initial: Option<Vec<Conserved>>,
    mut observer: impl FnMut(&ConvergenceSample, &[Conserved]) -> Result<(), E>,
) -> Result<Solution, ObservationError<E>> {
    validate(config)?;
    let mut state = initial.unwrap_or_else(|| op.initial_state());
    let mut workspace = op.workspace();
    let mut stage = state.clone();
    let mut dt = vec![0.0; state.len()];
    let mut history = Vec::new();
    let mut previous_wall = op.wall_samples(&state)?;
    let mut consecutive = 0_u32;
    for step in 1..=config.max_steps {
        let second_order = step > config.first_order_steps;
        op.evaluate(&state, second_order, &mut workspace)?;
        for i in 0..state.len() {
            dt[i] = config.cfl * workspace.time_step[i];
            stage[i] = std::array::from_fn(|k| state[i][k] + dt[i] * workspace.residual[i][k]);
        }
        op.evaluate(&stage, second_order, &mut workspace)?;
        for i in 0..state.len() {
            state[i] = std::array::from_fn(|k| {
                0.5 * (state[i][k] + stage[i][k] + dt[i] * workspace.residual[i][k])
            });
        }
        if step % config.check_interval == 0 || step == config.max_steps {
            op.evaluate(&state, second_order, &mut workspace)?;
            let wall = op.wall_samples(&state)?;
            let check = convergence(op, step, &workspace.residual, &wall, &previous_wall);
            observer(&check, &state).map_err(ObservationError::Observer)?;
            let passed = second_order
                && check.residual < config.residual_tolerance
                && check.heat_flux_change < config.heat_flux_tolerance;
            consecutive = if passed {
                consecutive.wrapping_add(1)
            } else {
                0
            };
            history.push(check);
            previous_wall = wall;
            if consecutive >= 3 {
                return Ok(Solution {
                    converged: true,
                    steps: step,
                    state,
                    wall: previous_wall,
                    history,
                });
            }
        }
    }
    Ok(Solution {
        converged: false,
        steps: config.max_steps,
        state,
        wall: previous_wall,
        history,
    })
}

fn validate(c: SolverConfig) -> Result<(), CfdError> {
    if !c.cfl.is_finite()
        || c.cfl <= 0.0
        || c.cfl > 0.8
        || c.max_steps == 0
        || c.check_interval == 0
        || c.first_order_steps >= c.max_steps
        || !c.residual_tolerance.is_finite()
        || c.residual_tolerance <= 0.0
        || !c.heat_flux_tolerance.is_finite()
        || c.heat_flux_tolerance <= 0.0
    {
        return Err(CfdError::Configuration(
            "invalid steady iteration controls".into(),
        ));
    }
    Ok(())
}

fn convergence(
    op: &FlowOperator,
    step: usize,
    residual: &[Conserved],
    wall: &[WallSample],
    previous: &[WallSample],
) -> ConvergenceSample {
    let speed = norm(op.inflow.velocity).max(op.gas.sound_speed(op.inflow));
    let rho = op.inflow.density;
    let scales = [
        rho,
        rho * speed,
        rho * speed,
        rho * speed,
        rho * speed * speed,
    ];
    let time = op.mesh.radius / speed;
    let rms = (residual
        .iter()
        .map(|r| {
            r.iter()
                .zip(scales)
                .map(|(v, s)| (v * time / s).powi(2))
                .sum::<f64>()
        })
        .sum::<f64>()
        / (5 * residual.len()) as f64)
        .sqrt();
    let peak_cell = residual
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                i,
                r.iter()
                    .zip(scales)
                    .map(|(v, s)| (v / s).abs())
                    .fold(0.0_f64, f64::max),
            )
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0, |v| v.0);
    let peak = wall
        .iter()
        .map(|w| w.heat_flux.abs())
        .fold(1.0_f64, f64::max);
    let change = wall
        .iter()
        .zip(previous)
        .map(|(a, b)| (a.heat_flux - b.heat_flux).abs() / peak)
        .fold(0.0_f64, f64::max);
    ConvergenceSample {
        step,
        residual: rms,
        peak_cell,
        heat_flux_change: change,
        nose_heat_flux: wall.first().map_or(0.0, |w| w.heat_flux),
        integrated_heating: wall.iter().map(|w| w.heat_flux * w.area).sum(),
    }
}

#[cfg(test)]
mod tests;
