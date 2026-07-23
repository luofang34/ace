use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::Diagnostic;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChartSpec {
    pub(crate) chart_type: String,
    pub(crate) title: String,
    pub(crate) x: AxisSpec,
    pub(crate) y: AxisSpec,
    pub(crate) series: Vec<SeriesSpec>,
    pub(crate) annotations: Vec<Annotation>,
    pub(crate) warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AxisSpec {
    pub(crate) label: String,
    pub(crate) unit: String,
    pub(crate) values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SeriesSpec {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) unit: String,
    pub(crate) values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Annotation {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) label: String,
}
