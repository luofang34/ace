use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::schema::{ResolvedScenario, Wing};

const INNER_SEMISPAN_FRACTION: f64 = 0.23;
const TIP_CHORD_RATIO: f64 = 0.16;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlendedWingPlanform {
    pub(crate) inner_span_m: f64,
    pub(crate) outer_span_m: f64,
    pub(crate) root_chord_m: f64,
    pub(crate) middle_chord_m: f64,
    pub(crate) tip_chord_m: f64,
    pub(crate) center_edge_sweep_rad: f64,
    pub(crate) center_quarter_sweep_rad: f64,
    pub(crate) outer_quarter_sweep_rad: f64,
}

#[derive(Debug, Clone, Copy)]
struct PlanformPanel {
    span_m: f64,
    root_chord_m: f64,
    tip_chord_m: f64,
    root_quarter_x_m: f64,
    tip_quarter_x_m: f64,
}

impl BlendedWingPlanform {
    pub(crate) fn from_wing(wing: &Wing) -> AexResult<Self> {
        let semispan = wing.span_m * 0.5;
        let inner_span = semispan * INNER_SEMISPAN_FRACTION;
        let outer_span = semispan - inner_span;
        let (root_chord, middle_chord, edge_sweep, quarter_sweep) =
            match wing.center_body_edge_sweep_rad {
                Some(edge_sweep) => edge_swept_center(wing, inner_span, outer_span, edge_sweep)?,
                None => default_center(wing, inner_span, outer_span),
            };
        Ok(Self {
            inner_span_m: inner_span,
            outer_span_m: outer_span,
            root_chord_m: root_chord,
            middle_chord_m: middle_chord,
            tip_chord_m: middle_chord * TIP_CHORD_RATIO,
            center_edge_sweep_rad: edge_sweep,
            center_quarter_sweep_rad: quarter_sweep,
            outer_quarter_sweep_rad: wing.sweep_quarter_chord_rad,
        })
    }

    pub(crate) fn mean_aerodynamic_chord_m(self) -> f64 {
        let panels = self.panels();
        panels
            .iter()
            .copied()
            .map(PlanformPanel::chord_square_integral_m3)
            .sum::<f64>()
            / self.half_area_m2()
    }

    pub(crate) fn center_of_gravity_x_m(self) -> f64 {
        let quarter_chord_x = self
            .panels()
            .iter()
            .copied()
            .map(PlanformPanel::chord_quarter_x_integral_m3)
            .sum::<f64>()
            / self.half_area_m2();
        quarter_chord_x + 0.05 * self.mean_aerodynamic_chord_m()
    }

    pub(crate) fn edge_alignment_error_rad(self) -> f64 {
        let leading_edge_sweep = self.center_edge_sweep_rad;
        let trailing_edge_sweep = -self.center_edge_sweep_rad;
        (leading_edge_sweep + trailing_edge_sweep).abs()
    }

    pub(crate) fn independent_planform_angle_count(self) -> u32 {
        2
    }

    pub(crate) fn outer_panel_area_m2(self) -> f64 {
        2.0 * self.panels()[1].area_m2()
    }

    pub(crate) fn outer_panel_lift_centroid_m(self) -> f64 {
        self.outer_span_m * (self.middle_chord_m + 2.0 * self.tip_chord_m)
            / (3.0 * (self.middle_chord_m + self.tip_chord_m))
    }

    pub(crate) fn estimated_usable_volume_m3(self) -> f64 {
        let center = panel_volume(
            self.inner_span_m,
            self.root_chord_m,
            self.middle_chord_m,
            0.16,
            0.13,
        );
        let outer = panel_volume(
            self.outer_span_m,
            self.middle_chord_m,
            self.tip_chord_m,
            0.13,
            0.10,
        );
        2.0 * 0.45 * (center + outer)
    }

    fn half_area_m2(self) -> f64 {
        self.panels()
            .iter()
            .copied()
            .map(PlanformPanel::area_m2)
            .sum()
    }

    fn panels(self) -> [PlanformPanel; 2] {
        let inner_root_quarter_x = 0.25 * self.root_chord_m;
        let inner_tip_quarter_x =
            inner_root_quarter_x + self.inner_span_m * self.center_quarter_sweep_rad.tan();
        let outer_tip_quarter_x =
            inner_tip_quarter_x + self.outer_span_m * self.outer_quarter_sweep_rad.tan();
        [
            PlanformPanel {
                span_m: self.inner_span_m,
                root_chord_m: self.root_chord_m,
                tip_chord_m: self.middle_chord_m,
                root_quarter_x_m: inner_root_quarter_x,
                tip_quarter_x_m: inner_tip_quarter_x,
            },
            PlanformPanel {
                span_m: self.outer_span_m,
                root_chord_m: self.middle_chord_m,
                tip_chord_m: self.tip_chord_m,
                root_quarter_x_m: inner_tip_quarter_x,
                tip_quarter_x_m: outer_tip_quarter_x,
            },
        ]
    }
}

impl PlanformPanel {
    fn area_m2(self) -> f64 {
        0.5 * self.span_m * (self.root_chord_m + self.tip_chord_m)
    }

    fn chord_square_integral_m3(self) -> f64 {
        self.span_m
            * (self.root_chord_m.powi(2)
                + self.root_chord_m * self.tip_chord_m
                + self.tip_chord_m.powi(2))
            / 3.0
    }

    fn chord_quarter_x_integral_m3(self) -> f64 {
        self.span_m
            * (2.0 * self.root_chord_m * self.root_quarter_x_m
                + self.root_chord_m * self.tip_quarter_x_m
                + self.tip_chord_m * self.root_quarter_x_m
                + 2.0 * self.tip_chord_m * self.tip_quarter_x_m)
            / 6.0
    }
}

pub(crate) fn is_blended_wing_body(scenario: &ResolvedScenario) -> bool {
    let topology = &scenario.aircraft.topology;
    topology.has_component_kind("lifting_body")
        || topology.inferred
            && crate::domain::topology::legacy_lifting_body(&scenario.aircraft.configuration)
}

fn edge_swept_center(
    wing: &Wing,
    inner_span: f64,
    outer_span: f64,
    edge_sweep: f64,
) -> AexResult<(f64, f64, f64, f64)> {
    if !edge_sweep.is_finite() || edge_sweep <= 0.0 || edge_sweep >= 90_f64.to_radians() {
        return Err(AexError::validation(
            "INVALID_CENTER_BODY_EDGE_SWEEP",
            "aircraft.geometry.wing.center_body_edge_sweep",
            "edge sweep must be finite and between 0 and 90 degrees",
        ));
    }
    let chord_delta = 2.0 * inner_span * edge_sweep.tan();
    let denominator = 2.0 * inner_span + outer_span * (1.0 + TIP_CHORD_RATIO);
    let middle_chord = (wing.area_m2 - inner_span * chord_delta) / denominator;
    if middle_chord <= 0.0 {
        return Err(AexError::validation(
            "CENTER_BODY_PLANFORM_DOES_NOT_CLOSE",
            "aircraft.geometry.wing.center_body_edge_sweep",
            "requested edge sweep consumes the available wing area",
        ));
    }
    let root_chord = middle_chord + chord_delta;
    let quarter_sweep = (0.5 * edge_sweep.tan()).atan();
    Ok((root_chord, middle_chord, edge_sweep, quarter_sweep))
}

fn default_center(wing: &Wing, inner_span: f64, outer_span: f64) -> (f64, f64, f64, f64) {
    let middle_ratio = 0.55;
    let denominator =
        inner_span * (1.0 + middle_ratio) + outer_span * middle_ratio * (1.0 + TIP_CHORD_RATIO);
    let root_chord = wing.area_m2 / denominator;
    let middle_chord = root_chord * middle_ratio;
    let chord_delta = root_chord - middle_chord;
    let edge_sweep = (0.5 * chord_delta / inner_span).atan();
    let quarter_sweep = (0.25 * chord_delta / inner_span).atan();
    (root_chord, middle_chord, edge_sweep, quarter_sweep)
}

fn panel_volume(span: f64, root_chord: f64, tip_chord: f64, root_tc: f64, tip_tc: f64) -> f64 {
    let section = |fraction: f64| {
        let chord = root_chord + fraction * (tip_chord - root_chord);
        let thickness_ratio = root_tc + fraction * (tip_tc - root_tc);
        0.65 * thickness_ratio * chord.powi(2)
    };
    span * (section(0.0) + 4.0 * section(0.5) + section(1.0)) / 6.0
}

#[cfg(test)]
mod tests;
