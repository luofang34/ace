use clap::{ArgMatches, Error, error::ErrorKind};
use std::path::PathBuf;

use super::arguments::*;

pub(super) fn command(matches: ArgMatches) -> Result<Command, Error> {
    let (name, args) = subcommand(&matches)?;
    match name {
        "drawing" => drawing(args),
        "capabilities" => Ok(Command::Capabilities {
            output: output(args)?,
        }),
        "validate" => Ok(Command::Validate {
            path: required(args, "path")?,
            output: output(args)?,
        }),
        "resolve" => Ok(Command::Resolve {
            scenario: required(args, "scenario")?,
            common: common(args)?,
        }),
        "synthesize" => Ok(Command::Synthesize(ScenarioArgs {
            scenario: required(args, "scenario")?,
            common: common(args)?,
        })),
        "sheet" => Ok(Command::Sheet {
            scenario: required(args, "scenario")?,
            port: required(args, "port")?,
            common: common(args)?,
        }),
        "study" => Ok(Command::Study {
            scenario: required(args, "scenario")?,
            plan: required(args, "plan")?,
            chart: optional(args, "chart"),
            artifact: optional(args, "artifact"),
            common: common(args)?,
        }),
        "edit" => Ok(Command::Edit {
            scenario: required(args, "scenario")?,
            edits: optional(args, "edits"),
            expected_revision: optional(args, "expected-revision"),
            save_as: optional(args, "save-as"),
            common: common(args)?,
        }),
        "promote" => Ok(Command::Promote {
            archive: required(args, "archive")?,
            candidate: required(args, "candidate")?,
            save_as: required(args, "save-as")?,
            output: output(args)?,
        }),
        "explain" => Ok(Command::Explain {
            scenario: required(args, "scenario")?,
            parameter: required(args, "parameter")?,
            depth: required(args, "depth")?,
            common: common(args)?,
        }),
        "payload-range" => Ok(Command::PayloadRange(PayloadRangeArgs {
            vehicle: required(args, "vehicle")?,
            distance: required(args, "distance")?,
            remaining: required(args, "remaining")?,
            maximum_takeoff_mass: required(args, "maximum-takeoff-mass")?,
            maximum_payload: required(args, "maximum-payload")?,
            artifact: optional(args, "artifact"),
            output: output(args)?,
        })),
        "catalog" => Ok(Command::Catalog {
            vehicle: required(args, "vehicle")?,
            entry: optional(args, "entry"),
            output: output(args)?,
        }),
        "chart" => Ok(Command::Chart {
            vehicle: required(args, "vehicle")?,
            analysis: required(args, "analysis")?,
            artifact: optional(args, "artifact"),
            common: common(args)?,
        }),
        "uncertainty" => Ok(Command::Uncertainty {
            scenario: required(args, "scenario")?,
            plan: required(args, "plan")?,
            common: common(args)?,
        }),
        "calibrate" => Ok(Command::Calibrate {
            scenario: required(args, "scenario")?,
            case: required(args, "case")?,
            common: common(args)?,
        }),
        "campaign" => Ok(Command::Campaign {
            campaign: required(args, "campaign")?,
            output: output(args)?,
        }),
        "analyze" => Ok(Command::Analyze {
            analysis: analysis(args)?,
        }),
        "compare" => Ok(Command::Compare(CompareArgs {
            vehicles: many(args, "vehicles"),
            metrics: many(args, "metric"),
            output: output(args)?,
        })),
        "profile" => Ok(Command::Profile {
            profile: profile(args)?,
        }),
        "plot" => Ok(Command::Plot(plot(args)?)),
        "report" => Ok(Command::Report(ReportArgs {
            run_id: optional(args, "run_id"),
            vehicle: optional(args, "vehicle"),
            format: required(args, "format")?,
            output: OutputArgs {
                format: OutputFormat::Json,
                output: optional(args, "output"),
                units: optional(args, "units"),
            },
        })),
        "mcp" => Ok(Command::Mcp {
            mcp: McpCommand::Serve,
        }),
        _ => Err(invalid(format!("unsupported command {name}"))),
    }
}

fn drawing(args: &ArgMatches) -> Result<Command, Error> {
    Ok(Command::Drawing(DrawingArgs {
        source: required(args, "source")?,
        directory: required(args, "directory")?,
        paper: match required::<String>(args, "paper")?.as_str() {
            "A1" => ace_drawing::Paper::A1,
            "16:9" => ace_drawing::Paper::Widescreen,
            _ => ace_drawing::Paper::A3,
        },
        plan: optional(args, "plan"),
        candidate: optional(args, "candidate"),
        scenario: required(args, "mission")?,
        replay: args.get_flag("replay"),
        drawing_data: optional(args, "drawing-data"),
        from_snapshot: args.get_flag("from-snapshot"),
        branding: (args.contains_id("brand") || args.contains_id("drawing-title")).then(|| {
            ace_drawing::Branding {
                organization: optional(args, "brand").unwrap_or_default(),
                title: optional(args, "drawing-title")
                    .unwrap_or_else(|| ace_drawing::Branding::default().title),
            }
        }),
        common: common(args)?,
    }))
}

fn analysis(matches: &ArgMatches) -> Result<AnalyzeCommand, Error> {
    let (name, args) = subcommand(matches)?;
    let scenario: PathBuf = required(args, "scenario")?;
    let common = common(args)?;
    match name {
        "point" => Ok(AnalyzeCommand::Point(PointArgs {
            scenario,
            common,
            altitude: required(args, "altitude")?,
            speed: optional(args, "speed"),
            mach: optional(args, "mach"),
            mass: optional(args, "mass"),
            cg: optional(args, "cg"),
            configuration: required(args, "configuration")?,
        })),
        "performance" => Ok(AnalyzeCommand::Performance(PerformanceArgs {
            scenario,
            mass: optional(args, "mass"),
            landing_mass: optional(args, "landing_mass"),
            common,
        })),
        _ => Err(invalid(format!("unsupported analysis {name}"))),
    }
}

fn profile(matches: &ArgMatches) -> Result<ProfileCommand, Error> {
    let (name, args) = subcommand(matches)?;
    let output = output(args)?;
    match name {
        "list" => Ok(ProfileCommand::List {
            directory: required(args, "directory")?,
            profile_type: optional(args, "profile_type"),
            query: optional(args, "query"),
            output,
        }),
        "show" => Ok(ProfileCommand::Show {
            profile_id: required(args, "profile_id")?,
            directory: required(args, "directory")?,
            output,
        }),
        "validate" => Ok(ProfileCommand::Validate {
            path: required(args, "path")?,
            output,
        }),
        _ => Err(invalid(format!("unsupported profile command {name}"))),
    }
}

fn plot(args: &ArgMatches) -> Result<PlotArgs, Error> {
    let kind = match required::<String>(args, "kind")?.as_str() {
        "drag-polar" => PlotKind::DragPolar,
        "power-curves" => PlotKind::PowerCurves,
        "thrust-curves" => PlotKind::ThrustCurves,
        "climb-envelope" => PlotKind::ClimbEnvelope,
        "mission-mass" => PlotKind::MissionMass,
        "requirement-margins" => PlotKind::RequirementMargins,
        value => return Err(invalid(format!("unknown plot kind {value}"))),
    };
    Ok(PlotArgs {
        kind,
        scenario: required(args, "scenario")?,
        artifact: optional(args, "output"),
        format: format_for_path(args, optional::<PathBuf>(args, "spec-output").as_deref())?,
        spec_output: optional(args, "spec-output"),
        units: optional(args, "units"),
        altitude: required(args, "altitude")?,
        strict: args.get_flag("strict"),
        seed: required(args, "seed")?,
        overrides: overrides(args)?,
    })
}

fn common(args: &ArgMatches) -> Result<CommonArgs, Error> {
    Ok(CommonArgs {
        output: output(args)?,
        strict: args.get_flag("strict"),
        seed: required(args, "seed")?,
        overrides: overrides(args)?,
    })
}

fn overrides(args: &ArgMatches) -> Result<Vec<(String, String)>, Error> {
    many::<String>(args, "set")
        .iter()
        .map(|value| {
            value
                .split_once('=')
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .ok_or_else(|| invalid("override must use PATH=VALUE"))
        })
        .collect()
}

fn output(args: &ArgMatches) -> Result<OutputArgs, Error> {
    let path: Option<PathBuf> = optional(args, "output");
    Ok(OutputArgs {
        format: format_for_path(args, path.as_deref())?,
        output: path,
        units: optional(args, "units"),
    })
}

fn format_for_path(
    args: &ArgMatches,
    path: Option<&std::path::Path>,
) -> Result<OutputFormat, Error> {
    let extension = path.and_then(|p| p.extension()).and_then(|e| e.to_str());
    let inferred = match extension {
        Some("json") => Some(OutputFormat::Json),
        Some("yaml" | "yml") => Some(OutputFormat::Yaml),
        Some("csv") => Some(OutputFormat::Csv),
        _ => None,
    };
    let selected = format(args)?;
    let explicit = args.value_source("format") == Some(clap::parser::ValueSource::CommandLine);
    if explicit && inferred.is_some_and(|f| f != selected) {
        return Err(invalid("output extension conflicts with explicit --format"));
    }
    Ok(if explicit {
        selected
    } else {
        inferred.unwrap_or(selected)
    })
}

fn format(args: &ArgMatches) -> Result<OutputFormat, Error> {
    match required::<String>(args, "format")?.as_str() {
        "table" => Ok(OutputFormat::Table),
        "json" => Ok(OutputFormat::Json),
        "yaml" => Ok(OutputFormat::Yaml),
        "csv" => Ok(OutputFormat::Csv),
        value => Err(invalid(format!("unsupported format {value}"))),
    }
}

fn subcommand(matches: &ArgMatches) -> Result<(&str, &ArgMatches), Error> {
    matches
        .subcommand()
        .ok_or_else(|| Error::raw(ErrorKind::MissingSubcommand, "a subcommand is required"))
}

fn optional<T: Clone + Send + Sync + 'static>(args: &ArgMatches, name: &str) -> Option<T> {
    args.get_one::<T>(name).cloned()
}

fn required<T: Clone + Send + Sync + 'static>(args: &ArgMatches, name: &str) -> Result<T, Error> {
    optional(args, name).ok_or_else(|| {
        Error::raw(
            ErrorKind::MissingRequiredArgument,
            format!("missing {name}"),
        )
    })
}

fn many<T: Clone + Send + Sync + 'static>(args: &ArgMatches, name: &str) -> Vec<T> {
    args.get_many::<T>(name)
        .map(|values| values.cloned().collect())
        .unwrap_or_default()
}

fn invalid(message: impl Into<String>) -> Error {
    Error::raw(ErrorKind::ValueValidation, message.into())
}
