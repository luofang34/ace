#[test]
fn command_tree_has_consistent_arguments() {
    super::command().debug_assert();
}

#[test]
fn output_extensions_select_the_serialization_and_conflicting_formats_fail() {
    use crate::cli::arguments::{Command, OutputFormat};
    let parse = |tail: &[&str]| {
        let mut args = vec!["aex", "synthesize", "model.json", "--output", "result.json"];
        args.extend_from_slice(tail);
        super::super::parsing::command(super::command().try_get_matches_from(args).unwrap())
    };
    match parse(&[]).unwrap() {
        Command::Synthesize(args) => assert_eq!(args.common.output.format, OutputFormat::Json),
        _ => panic!("expected synthesis"),
    }
    assert!(parse(&["--format", "yaml"]).is_err());
    assert!(parse(&["--format", "json"]).is_ok());
    let matches = super::command()
        .try_get_matches_from([
            "aex",
            "plot",
            "requirement-margins",
            "model.yaml",
            "--spec-output",
            "margins.json",
        ])
        .unwrap();
    match super::super::parsing::command(matches).unwrap() {
        Command::Plot(args) => assert_eq!(args.format, OutputFormat::Json),
        _ => panic!("expected plot"),
    }
}

#[test]
fn commands_parse_using_the_public_grammar() {
    for arguments in [
        vec![
            "aex",
            "analyze",
            "mission",
            "scenario.yaml",
            "--format",
            "json",
        ],
        vec![
            "aex",
            "study",
            "vehicle.json",
            "--plan",
            "plan.json",
            "--chart",
            "economics.casm",
            "--artifact",
            "casm.svg",
        ],
        vec!["aex", "report", "run-id", "--format", "markdown"],
        vec!["aex", "profile", "list"],
    ] {
        let matches = super::command()
            .try_get_matches_from(arguments)
            .expect("valid arguments");
        super::super::parsing::command(matches).expect("typed arguments");
    }
}
