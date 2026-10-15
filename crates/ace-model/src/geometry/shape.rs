use crate::{Binding, Unit};
use serde::{Deserialize, Serialize};

/// Parameterized conceptual surfaces, without detailed structure or certified airfoils.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConceptShape {
    /// Elliptical constant section with ellipsoidal nose and tail, along local +x.
    Body {
        /// Total axial length.
        length: Binding,
        /// Maximum full width.
        width: Binding,
        /// Maximum full height.
        height: Binding,
        /// Ellipsoidal forebody length.
        nose: Binding,
        /// Ellipsoidal afterbody length.
        tail: Binding,
    },
    /// Trapezoidal planform with symmetric diamond sections; root leading edge is the origin.
    Wing {
        /// Full span for symmetric surfaces, exposed span for one-sided fins.
        span: Binding,
        /// Total projected planform area.
        area: Binding,
        /// Tip/root chord ratio.
        taper: Binding,
        /// Leading-edge sweep.
        sweep: Binding,
        /// Dihedral relative to local x/y plane.
        dihedral: Binding,
        /// Maximum thickness/chord ratio.
        thickness: Binding,
        /// Mirror across local y=0; false models a single positive-y surface.
        symmetric: bool,
    },
}

impl ConceptShape {
    pub(super) fn bindings(&self) -> Vec<(&Binding, Unit)> {
        use Unit::*;
        match self {
            Self::Body {
                length,
                width,
                height,
                nose,
                tail,
            } => [length, width, height, nose, tail]
                .into_iter()
                .map(|b| (b, Meter))
                .collect(),
            Self::Wing {
                span,
                area,
                taper,
                sweep,
                dihedral,
                thickness,
                ..
            } => vec![
                (span, Meter),
                (area, SquareMeter),
                (taper, Ratio),
                (sweep, Radian),
                (dihedral, Radian),
                (thickness, Ratio),
            ],
        }
    }
}
