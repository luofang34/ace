use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug)]
pub(super) enum Command {
    Capabilities {
        output: OutputArgs,
    },
    Validate {
        path: PathBuf,
        output: OutputArgs,
    },
    Resolve {
        scenario: PathBuf,
        common: CommonArgs,
    },
    Analyze {
        analysis: AnalyzeCommand,
    },
    Sweep(SweepArgs),
    Compare(CompareArgs),
    Profile {
        profile: ProfileCommand,
    },
    Plot(PlotArgs),
    Report(ReportArgs),
    Synthesize(ScenarioArgs),
    Drawing(DrawingArgs),
    Study {
        scenario: PathBuf,
        plan: PathBuf,
        common: CommonArgs,
    },
    Sheet {
        scenario: PathBuf,
        port: u16,
        common: CommonArgs,
    },
    Mcp {
        mcp: McpCommand,
    },
}

#[derive(Debug)]
pub(super) struct DrawingArgs {
    pub source: PathBuf,
    pub directory: PathBuf,
    pub paper: ace_drawing::Paper,
    pub plan: Option<PathBuf>,
    pub candidate: Option<usize>,
    pub scenario: String,
    pub replay: bool,
    pub branding: Option<ace_drawing::Branding>,
    pub drawing_data: Option<PathBuf>,
    pub from_snapshot: bool,
    pub common: CommonArgs,
}

#[derive(Debug)]
pub(super) enum AnalyzeCommand {
    Point(PointArgs),
    Mission(ScenarioArgs),
    Constraints(ConstraintArgs),
    PayloadRange(ScenarioArgs),
    Performance(PerformanceArgs),
}

#[derive(Debug)]
pub(super) struct PointArgs {
    pub(super) scenario: PathBuf,
    pub(super) altitude: String,
    pub(super) speed: Option<String>,
    pub(super) mach: Option<f64>,
    pub(super) mass: Option<String>,
    pub(super) configuration: String,
    pub(super) common: CommonArgs,
}

#[derive(Debug)]
pub(super) struct PerformanceArgs {
    pub(super) scenario: PathBuf,
    /// Takeoff mass for the summary and field lengths (`mission.field.takeoff_mass`).
    pub(super) mass: Option<String>,
    /// Landing mass for the landing field length (`mission.field.landing_mass`).
    pub(super) landing_mass: Option<String>,
    pub(super) common: CommonArgs,
}

#[derive(Debug)]
pub(super) struct ScenarioArgs {
    pub(super) scenario: PathBuf,
    pub(super) common: CommonArgs,
}

#[derive(Debug)]
pub(super) struct ConstraintArgs {
    pub(super) scenario: PathBuf,
    pub(super) start: String,
    pub(super) stop: String,
    pub(super) count: u32,
    pub(super) common: CommonArgs,
}

#[derive(Debug)]
pub(super) struct SweepArgs {
    pub(super) scenario: PathBuf,
    pub(super) variables: Vec<String>,
    pub(super) metrics: Vec<String>,
    pub(super) logarithmic: bool,
    pub(super) common: CommonArgs,
}

#[derive(Debug)]
pub(super) struct CompareArgs {
    pub(super) scenarios: Vec<PathBuf>,
    pub(super) metrics: Vec<String>,
    pub(super) strict: bool,
    pub(super) output: OutputArgs,
}

#[derive(Debug)]
pub(super) enum ProfileCommand {
    List {
        directory: PathBuf,
        profile_type: Option<String>,
        query: Option<String>,
        output: OutputArgs,
    },
    Show {
        profile_id: String,
        directory: PathBuf,
        output: OutputArgs,
    },
    Validate {
        path: PathBuf,
        output: OutputArgs,
    },
}

#[derive(Debug)]
pub(super) struct PlotArgs {
    pub(super) kind: PlotKind,
    pub(super) scenario: PathBuf,
    pub(super) artifact: Option<PathBuf>,
    pub(super) format: OutputFormat,
    pub(super) spec_output: Option<PathBuf>,
    pub(super) units: Option<String>,
    pub(super) altitude: String,
    pub(super) strict: bool,
    pub(super) seed: u64,
    pub(super) overrides: Vec<(String, String)>,
}

impl PlotArgs {
    pub(super) fn override_map(&self) -> BTreeMap<String, String> {
        self.overrides.iter().cloned().collect()
    }

    pub(super) fn output_args(&self) -> OutputArgs {
        OutputArgs {
            format: self.format,
            output: self.spec_output.clone(),
            units: self.units.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum PlotKind {
    DragPolar,
    PowerCurves,
    ThrustCurves,
    ClimbEnvelope,
    PayloadRange,
    Constraints,
    MissionMass,
    RequirementMargins,
}

impl PlotKind {
    pub(super) fn file_stem(self) -> &'static str {
        match self {
            Self::DragPolar => "drag-polar",
            Self::PowerCurves => "power-curves",
            Self::ThrustCurves => "thrust-curves",
            Self::ClimbEnvelope => "climb-envelope",
            Self::PayloadRange => "payload-range",
            Self::Constraints => "constraints",
            Self::MissionMass => "mission-mass",
            Self::RequirementMargins => "requirement-margins",
        }
    }
}

#[derive(Debug)]
pub(super) struct ReportArgs {
    pub(super) run_id: String,
    pub(super) format: String,
    pub(super) output: OutputArgs,
}

#[derive(Debug)]
pub(super) enum McpCommand {
    Serve,
}

#[derive(Debug, Clone)]
pub(super) struct CommonArgs {
    pub(super) output: OutputArgs,
    pub(super) strict: bool,
    pub(super) seed: u64,
    pub(super) overrides: Vec<(String, String)>,
}

impl CommonArgs {
    pub(super) fn override_map(&self) -> BTreeMap<String, String> {
        self.overrides.iter().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub(super) struct OutputArgs {
    pub(super) format: OutputFormat,
    pub(super) output: Option<PathBuf>,
    pub(super) units: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputFormat {
    Table,
    Json,
    Yaml,
    Csv,
}
