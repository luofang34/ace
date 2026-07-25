use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

pub(crate) mod commands;
mod failure;
mod output;
mod plots;

#[derive(Debug, Parser)]
#[command(name = "aircraft-explorer", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Capabilities {
        #[command(flatten)]
        output: OutputArgs,
    },
    Validate {
        path: PathBuf,
        #[command(flatten)]
        output: OutputArgs,
    },
    Resolve {
        scenario: PathBuf,
        #[command(flatten)]
        common: CommonArgs,
    },
    Analyze {
        #[command(subcommand)]
        analysis: AnalyzeCommand,
    },
    Sweep(SweepArgs),
    Compare(CompareArgs),
    Profile {
        #[command(subcommand)]
        profile: ProfileCommand,
    },
    Plot(PlotArgs),
    Report(ReportArgs),
    Mcp {
        #[command(subcommand)]
        mcp: McpCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AnalyzeCommand {
    Point(PointArgs),
    Mission(ScenarioArgs),
    Constraints(ConstraintArgs),
    PayloadRange(ScenarioArgs),
    Performance(ScenarioArgs),
}

#[derive(Debug, Args)]
struct PointArgs {
    scenario: PathBuf,
    #[arg(long)]
    altitude: String,
    #[arg(long)]
    speed: Option<String>,
    #[arg(long)]
    mach: Option<f64>,
    #[arg(long)]
    mass: Option<String>,
    #[arg(long, default_value = "clean")]
    configuration: String,
    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Debug, Args)]
struct ScenarioArgs {
    scenario: PathBuf,
    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Debug, Args)]
struct ConstraintArgs {
    scenario: PathBuf,
    #[arg(long, default_value = "300 N/m^2")]
    start: String,
    #[arg(long, default_value = "9000 N/m^2")]
    stop: String,
    #[arg(long, default_value_t = 80)]
    count: u32,
    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Debug, Args)]
struct SweepArgs {
    scenario: PathBuf,
    #[arg(long = "var", required = true)]
    variables: Vec<String>,
    #[arg(long = "metric", required = true)]
    metrics: Vec<String>,
    #[arg(long)]
    logarithmic: bool,
    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Debug, Args)]
struct CompareArgs {
    #[arg(required = true)]
    scenarios: Vec<PathBuf>,
    #[arg(long = "metric", required = true)]
    metrics: Vec<String>,
    #[arg(long)]
    strict: bool,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Subcommand)]
enum ProfileCommand {
    List {
        #[arg(long, default_value = "profiles")]
        directory: PathBuf,
        #[arg(long)]
        profile_type: Option<String>,
        #[arg(long)]
        query: Option<String>,
        #[command(flatten)]
        output: OutputArgs,
    },
    Show {
        profile_id: String,
        #[arg(long, default_value = "profiles")]
        directory: PathBuf,
        #[command(flatten)]
        output: OutputArgs,
    },
    Validate {
        path: PathBuf,
        #[command(flatten)]
        output: OutputArgs,
    },
}

#[derive(Debug, Args)]
struct PlotArgs {
    kind: PlotKind,
    scenario: PathBuf,
    #[arg(long = "output")]
    artifact: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,
    #[arg(long)]
    spec_output: Option<PathBuf>,
    #[arg(long)]
    units: Option<String>,
    #[arg(long, default_value = "0 m")]
    altitude: String,
    #[arg(long)]
    strict: bool,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long = "set", value_parser = parse_override)]
    overrides: Vec<(String, String)>,
}

impl PlotArgs {
    fn override_map(&self) -> BTreeMap<String, String> {
        self.overrides.iter().cloned().collect()
    }

    fn output_args(&self) -> OutputArgs {
        OutputArgs {
            format: self.format,
            output: self.spec_output.clone(),
            units: self.units.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PlotKind {
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
    fn file_stem(self) -> &'static str {
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

#[derive(Debug, Args)]
struct ReportArgs {
    run_id: String,
    #[arg(long, default_value = "markdown")]
    format: String,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    Serve,
}

#[derive(Debug, Clone, Args)]
struct CommonArgs {
    #[command(flatten)]
    output: OutputArgs,
    #[arg(long)]
    strict: bool,
    #[arg(long)]
    explain: bool,
    #[arg(long)]
    no_cache: bool,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long = "set", value_parser = parse_override)]
    overrides: Vec<(String, String)>,
    #[arg(long)]
    scenario_id: Option<String>,
}

impl CommonArgs {
    fn override_map(&self) -> BTreeMap<String, String> {
        self.overrides.iter().cloned().collect()
    }
}

#[derive(Debug, Clone, Args)]
struct OutputArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    units: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
    Yaml,
    Csv,
}

fn parse_override(raw: &str) -> Result<(String, String), String> {
    raw.split_once('=')
        .map(|(path, value)| (path.to_owned(), value.to_owned()))
        .ok_or_else(|| "override must use PATH=VALUE".to_owned())
}

/// Run the Aircraft Concept Explorer command-line interface.
pub async fn run_cli() -> ExitCode {
    drop(
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
            )
            .with_writer(std::io::stderr)
            .try_init(),
    );
    let arguments: Vec<OsString> = env::args_os().collect();
    let json = failure::json_requested(&arguments);
    let cli = match Cli::try_parse_from(&arguments) {
        Ok(cli) => cli,
        Err(error) if is_help(&error) => {
            return match error.print() {
                Ok(()) => ExitCode::SUCCESS,
                Err(source) => {
                    tracing::error!("failed to write CLI help: {source}");
                    ExitCode::FAILURE
                }
            };
        }
        Err(error) => {
            present_error(&failure::clap_detail(&error), &error.to_string(), json);
            return ExitCode::FAILURE;
        }
    };
    match commands::execute(cli.command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            present_error(&error.detail(), &error.to_string(), json);
            ExitCode::FAILURE
        }
    }
}

fn is_help(error: &clap::Error) -> bool {
    matches!(
        error.kind(),
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
    )
}

fn present_error(detail: &crate::domain::diagnostic::ErrorDetail, human_message: &str, json: bool) {
    if json {
        if let Err(source) = failure::emit_json(detail) {
            tracing::error!("failed to write structured error: {source}");
        }
    } else {
        tracing::error!("{human_message}");
    }
}
