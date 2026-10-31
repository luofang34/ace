use clap::{Arg, ArgAction, Command, value_parser};

pub(super) fn command() -> Command {
    Command::new("aircraft-explorer")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Vehicle synthesis and aircraft concept exploration")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(output(Command::new("capabilities")))
        .subcommand(output(Command::new("validate").arg(path("path"))))
        .subcommand(common(Command::new("resolve").arg(path("scenario"))))
        .subcommand(common(Command::new("synthesize").arg(path("scenario"))))
        .subcommand(drawing())
        .subcommand(common(
            Command::new("study")
                .about("Evaluate a vehicle parameter grid or bounded Pareto refinement")
                .arg(path("scenario"))
                .arg(optional_path("plan").required(true))
                .arg(Arg::new("chart").long("chart"))
                .arg(optional_path("artifact").requires("chart")),
        ))
        .subcommand(common(
            Command::new("edit")
                .about("Apply typed edits and parameter assignments as one validated revision")
                .arg(path("scenario"))
                .arg(optional_path("edits"))
                .arg(
                    Arg::new("expected-revision")
                        .long("expected-revision")
                        .value_parser(value_parser!(u64)),
                )
                .arg(optional_path("save-as")),
        ))
        .subcommand(output(
            Command::new("promote")
                .about("Write a verified, feasible study candidate as a new vehicle file")
                .arg(path("archive"))
                .arg(Arg::new("candidate").required(true))
                .arg(optional_path("save-as").required(true)),
        ))
        .subcommand(common(
            Command::new("explain")
                .about("Explain a vehicle parameter from its calculation evidence")
                .arg(path("scenario"))
                .arg(Arg::new("parameter").required(true))
                .arg(
                    Arg::new("depth")
                        .long("depth")
                        .default_value("2")
                        .value_parser(value_parser!(u32)),
                ),
        ))
        .subcommand(output(
            Command::new("payload-range")
                .about("Fly a vehicle's mission at its loading corners and solve each range")
                .arg(path("vehicle"))
                .arg(role(
                    "distance",
                    "Route-distance parameter the mission flies",
                ))
                .arg(role(
                    "remaining",
                    "Fuel-remaining parameter that reaches zero at the range",
                ))
                .arg(role(
                    "maximum-takeoff-mass",
                    "Maximum takeoff mass parameter",
                ))
                .arg(role("maximum-payload", "Maximum payload mass parameter"))
                .arg(optional_path("artifact")),
        ))
        .subcommand(common(
            Command::new("chart")
                .about("Chart one analysis of a synthesized vehicle from its run evidence")
                .arg(path("vehicle"))
                .arg(Arg::new("analysis").required(true))
                .arg(optional_path("artifact")),
        ))
        .subcommand(common(
            Command::new("uncertainty")
                .about("Propagate declared input uncertainty to requirement probabilities")
                .arg(path("scenario"))
                .arg(optional_path("plan").required(true)),
        ))
        .subcommand(common(
            Command::new("calibrate")
                .about("Fit bounded inputs to measured observations")
                .arg(path("scenario"))
                .arg(optional_path("case").required(true)),
        ))
        .subcommand(output(
            Command::new("campaign")
                .about("Compare fleets of synthesized vehicles on a persistent mission")
                .arg(path("campaign")),
        ))
        .subcommand(common(
            Command::new("sheet")
                .about("Open a local interactive synthesis sheet")
                .arg(path("scenario"))
                .arg(
                    Arg::new("port")
                        .long("port")
                        .default_value("0")
                        .value_parser(value_parser!(u16)),
                ),
        ))
        .subcommand(analysis())
        .subcommand(output(
            Command::new("compare")
                .about("Synthesize vehicles and tabulate named parameters with their verdicts")
                .arg(path("vehicles").num_args(1..))
                .arg(repeated("metric", true)),
        ))
        .subcommand(profiles())
        .subcommand(plot())
        .subcommand(
            Command::new("report")
                .arg(
                    Arg::new("run_id")
                        .required_unless_present("vehicle")
                        .conflicts_with("vehicle"),
                )
                .arg(optional_path("vehicle").help("Report on a synthesized vehicle file"))
                .arg(
                    Arg::new("format")
                        .long("format")
                        .default_value("markdown")
                        .value_parser(["markdown", "json"]),
                )
                .arg(optional_path("output"))
                .arg(Arg::new("units").long("units")),
        )
        .subcommand(
            Command::new("mcp")
                .subcommand_required(true)
                .subcommand(Command::new("serve")),
        )
}

fn drawing() -> Command {
    common(
        Command::new("drawing")
            .about("Compute and export a replayable single-page synthesis drawing")
            .arg(path("source"))
            .arg(optional_path("directory").required(true))
            .arg(
                Arg::new("paper")
                    .long("paper")
                    .value_parser(["A3", "A1", "16:9"])
                    .default_value("A3"),
            )
            .arg(optional_path("plan"))
            .arg(optional_path("drawing-data"))
            .arg(flag("from-snapshot"))
            .arg(
                Arg::new("candidate")
                    .long("candidate")
                    .value_parser(value_parser!(usize)),
            )
            .arg(Arg::new("mission").long("mission").default_value("design"))
            .arg(
                Arg::new("brand")
                    .long("brand")
                    .help("Customer organization in the drawing header and PDF metadata (ASCII)"),
            )
            .arg(
                Arg::new("drawing-title")
                    .long("drawing-title")
                    .help("Customer-facing drawing title (ASCII)"),
            )
            .arg(flag("replay")),
    )
}

fn analysis() -> Command {
    Command::new("analyze")
        .subcommand_required(true)
        .subcommand(common(
            Command::new("point")
                .arg(path("scenario"))
                .arg(Arg::new("altitude").long("altitude").required(true))
                .arg(Arg::new("speed").long("speed"))
                .arg(
                    Arg::new("mach")
                        .long("mach")
                        .value_parser(value_parser!(f64)),
                )
                .arg(Arg::new("mass").long("mass"))
                .arg(
                    Arg::new("cg")
                        .long("cg")
                        .help("Longitudinal CG station to trim at, from the geometry datum"),
                )
                .arg(
                    Arg::new("configuration")
                        .long("configuration")
                        .default_value("clean"),
                ),
        ))
        .subcommand(common(
            Command::new("performance")
                .arg(path("scenario"))
                .arg(
                    Arg::new("mass")
                        .long("mass")
                        .help("Takeoff mass for the summary and field lengths"),
                )
                .arg(
                    Arg::new("landing_mass")
                        .long("landing-mass")
                        .help("Landing mass for the landing field length"),
                ),
        ))
}

fn profiles() -> Command {
    Command::new("profile")
        .subcommand_required(true)
        .subcommand(output(
            Command::new("list")
                .arg(directory())
                .arg(Arg::new("profile_type").long("profile-type"))
                .arg(Arg::new("query").long("query")),
        ))
        .subcommand(output(
            Command::new("show")
                .arg(Arg::new("profile_id").required(true))
                .arg(directory()),
        ))
        .subcommand(output(Command::new("validate").arg(path("path"))))
}

fn plot() -> Command {
    Command::new("plot")
        .arg(Arg::new("kind").required(true).value_parser([
            "drag-polar",
            "power-curves",
            "thrust-curves",
            "climb-envelope",
            "mission-mass",
            "requirement-margins",
        ]))
        .arg(path("scenario"))
        .arg(optional_path("output"))
        .arg(optional_path("spec-output"))
        .arg(format())
        .arg(Arg::new("units").long("units"))
        .arg(Arg::new("altitude").long("altitude").default_value("0 m"))
        .arg(flag("strict"))
        .arg(seed())
        .arg(repeated("set", false))
}

fn output(command: Command) -> Command {
    command
        .arg(format())
        .arg(optional_path("output"))
        .arg(Arg::new("units").long("units"))
}

fn common(command: Command) -> Command {
    output(command)
        .arg(flag("strict"))
        .arg(flag("explain"))
        .arg(flag("no-cache"))
        .arg(seed())
        .arg(repeated("set", false))
        .arg(Arg::new("scenario-id").long("scenario-id"))
}

fn format() -> Arg {
    Arg::new("format")
        .long("format")
        .value_parser(["table", "json", "yaml", "csv"])
        .default_value("table")
}

fn seed() -> Arg {
    Arg::new("seed")
        .long("seed")
        .value_parser(value_parser!(u64))
        .default_value("0")
}

fn path(name: &'static str) -> Arg {
    Arg::new(name)
        .required(true)
        .value_parser(value_parser!(std::path::PathBuf))
}

fn optional_path(name: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .value_parser(value_parser!(std::path::PathBuf))
}

fn directory() -> Arg {
    optional_path("directory").default_value("profiles")
}

fn flag(name: &'static str) -> Arg {
    Arg::new(name).long(name).action(ArgAction::SetTrue)
}

fn role(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name).long(name).required(true).help(help)
}

fn repeated(name: &'static str, required: bool) -> Arg {
    Arg::new(name)
        .long(name)
        .required(required)
        .action(ArgAction::Append)
}

#[cfg(test)]
mod tests;
